use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

mod parse;

#[proc_macro]
pub fn tables(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as parse::Items);
    let code = input.iter().map(|item| match item {
        parse::Item::Single(item) => generate_item_code(item),
        parse::Item::Group(group) => generate_group(group),
    });
    quote! {
        #(#code)*
    }
    .into()
}

fn generate_item_code(item: &parse::SingleItem) -> proc_macro2::TokenStream {
    if item.fields.iter().all(|x| x.is_scalar()) {
        generate_zerocopy_impls(item)
    } else {
        generate_view_impls(item)
    }
}

fn generate_group(group: &parse::ItemGroup) -> proc_macro2::TokenStream {
    let name = &group.name;
    let lifetime = group.lifetime.as_ref().map(|_| quote!(<'a>));
    let variants = group.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = &variant.typ;
        let lifetime = variant.typ_lifetime.as_ref().map(|_| quote!(<'a>));
        quote!(#name(#typ #lifetime))
    });

    let format = &group.format_typ;
    let match_arms = group.variants.iter().map(|variant| {
        let name = &variant.name;
        let version = &variant.version;
        quote! {
            #version => {
                Some(Self::#name(raw_types::FontRead::read(bytes)?))
            }
        }
    });

    let var_versions = group.variants.iter().map(|v| &v.version);

    // make sure this is a constant and we aren't accidentally aliasing?
    // I'm not sure if this is necessary.
    let validation_check = quote! {
        #( const _: #format = #var_versions; )*
    };
    let font_read = quote! {

        impl<'a> raw_types::FontRead<'a> for #name #lifetime {
            fn read(bytes: &'a [u8]) -> Option<Self> {
                #validation_check
                let version: #format = raw_types::FontRead::read(bytes)?;
                match version {
                    #( #match_arms ),*

                        other => {
                            eprintln!("unknown enum variant {:?}", version);
                            None
                        }
                }
            }
        }
    };

    quote! {
        pub enum #name #lifetime {
            #( #variants ),*
        }

        #font_read
    }
}

fn generate_zerocopy_impls(item: &parse::SingleItem) -> proc_macro2::TokenStream {
    assert!(item.lifetime.is_none());
    let name = &item.name;
    let field_names = item
        .fields
        .iter()
        .map(|field| &field.as_scalar().unwrap().name);
    let field_types = item.fields.iter().map(parse::Field::concrete_type_tokens);

    quote! {
        #[derive(Clone, Copy, Debug, zerocopy::FromBytes, zerocopy::Unaligned)]
        #[repr(C)]
        pub struct #name {
            #( pub #field_names: #field_types, )*
        }

        // and now I want getters. but not for everything? also they return... a different concrete
        // type? or... do I want this, in the zero-copy version?
    }
}

fn generate_view_impls(item: &parse::SingleItem) -> proc_macro2::TokenStream {
    // scalars only get getters? that makes 'count' and friends complicated...
    // we can at least have a 'new' method that does a reasonable job of bounds checking,
    // but then we're going to be unsafing all over. that's also maybe okay though

    let name = &item.name;

    let mut current_offset = quote!(0);
    let mut in_checked_range = true;
    let mut checkable_len = 0;
    let mut getters = Vec::new();

    for field in &item.fields {
        match field {
            parse::Field::Scalar(scalar) => {
                if scalar.hidden.is_none() {
                    getters.push(make_scalar_getter(
                        scalar,
                        &current_offset,
                        in_checked_range,
                    ));
                }
                let field_len = scalar.typ.size();
                if in_checked_range {
                    checkable_len += field_len;
                }
                current_offset = quote!(#current_offset + #field_len);
            }
            parse::Field::Array(array) if array.variable_size.is_none() => {
                getters.push(make_array_getter(
                    array,
                    &mut current_offset,
                    in_checked_range,
                ));
                in_checked_range = false;
            }
            parse::Field::Array(array) => {
                getters.push(make_var_array_getter(array, &mut current_offset));
            }
        }
    }

    quote! {
        pub struct #name<'a>(&'a [u8]);

        impl<'a> raw_types::FontRead<'a> for #name<'a> {
            fn read(bytes: &'a [u8]) -> Option<Self> {
                if bytes.len() < #checkable_len {
                    return None;
                }
                Some(Self(bytes))
            }
        }

        impl<'a> #name<'a> {
            #( #getters )*
        }
    }
}

fn make_scalar_getter(
    field: &parse::ScalarField,
    offset: &proc_macro2::TokenStream,
    checked: bool,
) -> proc_macro2::TokenStream {
    let name = &field.name;
    let len = field.typ.size();

    let get_bytes = if checked {
        quote!(unsafe { self.0.get_unchecked(#offset..#offset + #len) })
    } else {
        quote!(self.0.get(#offset..#offset + #len).unwrap_or_default())
    };

    let ty = field.typ.raw_type_tokens();

    quote! {
        pub fn #name(&self) -> Option<#ty> {
            zerocopy::FromBytes::read_from(#get_bytes)
        }
    }
}

fn make_array_getter(
    field: &parse::ArrayField,
    offset: &mut proc_macro2::TokenStream,
    _checked: bool,
) -> proc_macro2::TokenStream {
    let name = &field.name;
    let start_off = offset.clone();
    let inner_typ = &field.inner_typ;
    assert!(
        field.inner_lifetime.is_none(),
        "inner_lifetime should only exist on variable size fields"
    );
    let len = match &field.count {
        parse::Count::Field(name) => Some(quote!(usize::from(self.#name().unwrap_or_default()))),
        parse::Count::Function { fn_, args } => {
            let args = args
                .iter()
                .map(|arg| quote!(self.#arg().unwrap_or_default()));
            Some(quote!(#fn_( #( #args ),* )))
        }
        parse::Count::Literal(lit) => Some(quote! { (#lit as usize) }),
        parse::Count::All(_) => None,
    };

    let range = match len {
        Some(len) => {
            *offset = quote!(#offset + #len);
            //FIXME: we need to figure out our 'get' business
            quote!(#start_off..#start_off + #len * std::mem::size_of::<#inner_typ>())
        }
        None => {
            // guard to ensure that this item is only ever the last:
            *offset = quote!(compile_error!(
                "#[count_all] annotation only valid on last field (TODO: validate before here)"
            ));
            quote!(#start_off..)
        }
    };
    quote! {
        pub fn #name(&self) -> Option<&'a [#inner_typ]> {
            self.0.get(#range)
                .and_then(|bytes| zerocopy::LayoutVerified::new_slice_unaligned(bytes))
                .map(|layout| layout.into_slice())
        }
    }
}

fn make_var_array_getter(
    field: &parse::ArrayField,
    offset: &mut proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let name = &field.name;
    let start_off = offset.clone();
    let inner_typ = &field.inner_typ;
    assert!(
        field.inner_lifetime.is_some(),
        "variable arrays are meaningless without an inner lifetime?"
    );
    *offset = quote!(compile_error!(
        "guard violated: variable_size array must be last field in item."
    ));
    quote! {
        pub fn #name(&self) -> Option<raw_types::VarArray<'a, #inner_typ<'a>>> {
            self.0.get(#start_off..).map(raw_types::VarArray::new)
        }
    }
}
