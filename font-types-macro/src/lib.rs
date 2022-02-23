use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse_macro_input;

mod parse;

#[proc_macro]
pub fn tables(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as parse::Items);
    let code = input.iter().map(|item| match item {
        parse::Item::Single(item) => generate_item_code(item),
        parse::Item::Group(group) => generate_group(group),
        parse::Item::RawEnum(raw_enum) => generate_raw_enum(raw_enum),
    });
    quote! {
        #(#code)*
    }
    .into()
}

fn generate_item_code(item: &parse::SingleItem) -> proc_macro2::TokenStream {
    if item.fields.iter().all(|x| x.is_single()) {
        generate_zerocopy_impls(item)
    } else {
        generate_view_impls(item)
    }
}

fn generate_group(group: &parse::ItemGroup) -> proc_macro2::TokenStream {
    let name = &group.name;
    let lifetime = group.lifetime.as_ref().map(|_| quote!(<'a>));
    let docs = &group.docs;
    let variants = group.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = &variant.typ;
        let docs = variant.docs.iter();
        let lifetime = variant.typ_lifetime.as_ref().map(|_| quote!(<'a>));
        quote! {
                #( #docs )*
                #name(#typ #lifetime)
        }
    });

    let format = &group.format_typ;
    let match_arms = group.variants.iter().map(|variant| {
        let name = &variant.name;
        let version = &variant.version;
        quote! {
            #version => {
                Some(Self::#name(font_types::FontRead::read(bytes)?))
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

        impl<'a> font_types::FontRead<'a> for #name #lifetime {
            fn read(bytes: &'a [u8]) -> Option<Self> {
                #validation_check
                let version: BigEndian<#format> = font_types::FontRead::read(bytes)?;
                match version.get() {
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
        #( #docs )*
        pub enum #name #lifetime {
            #( #variants ),*
        }

        #font_read
    }
}

fn generate_raw_enum(raw: &parse::RawEnum) -> proc_macro2::TokenStream {
    let name = &raw.name;
    let docs = &raw.docs;
    let repr = &raw.repr;
    let variants = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        let docs = &variant.docs;
        quote! {
            #( #docs )*
            #name = #value,
        }
    });
    let variant_inits = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        quote!(#value => Self::#name,)
    });

    quote! {
        #( #docs )*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(#repr)]
        pub enum #name {
            #( #variants )*
            Unknown,
        }

        impl #name {
            /// Create from a raw scalar.
            ///
            /// This will never fail; unknown values will be mapped to the `Unknown` variant
            pub fn new(raw: #repr) -> Self {
                match raw {
                    #( #variant_inits )*
                    _ => Self::Unknown,
                }
            }
        }
    }
}

fn generate_zerocopy_impls(item: &parse::SingleItem) -> proc_macro2::TokenStream {
    assert!(item.lifetime.is_none());
    let name = &item.name;
    let field_names = item
        .fields
        .iter()
        .map(|field| &field.as_single().unwrap().name);
    let docs = &item.docs;
    let field_types = item
        .fields
        .iter()
        .map(|field| &field.as_single().unwrap().typ);
    let field_docs = item
        .fields
        .iter()
        .map(|fld| {
            let docs = &fld.as_single().unwrap().docs;
            quote!( #( #docs )* )
        })
        .collect::<Vec<_>>();

    let getters = item
        .fields
        .iter()
        .map(|fld| generate_zc_getter(fld.as_single().unwrap()));

    quote! {
        #( #docs )*
        #[derive(Clone, Copy, Debug, zerocopy::FromBytes, zerocopy::Unaligned)]
        #[repr(C)]
        pub struct #name {
            #( #field_docs pub #field_names: #field_types, )*
        }

        impl #name {
            #(
                #field_docs
                #getters
            )*
        }

    }
}

fn generate_zc_getter(field: &parse::SingleField) -> proc_macro2::TokenStream {
    let name = &field.name;
    let cooked_type = field.cooked_type_tokens();
    if field.is_be_wrapper() {
        quote! {
            pub fn #name(&self) -> #cooked_type {
                self.#name.get()
            }
        }
    } else {
        quote! {
            pub fn #name(&self) -> &#cooked_type {
                &self.name
            }
        }
    }
}

fn generate_view_impls(item: &parse::SingleItem) -> proc_macro2::TokenStream {
    let name = &item.name;
    let docs = &item.docs;

    // these are fields which are inputs to the size calculations of *other fields.
    // For each of these field, we generate a 'resolved' identifier, and we assign the
    // value of this field to that identifier at init time. Subsequent fields can then
    // access that identifier in their own generated code, to determine the runtime
    // value of something.
    #[allow(clippy::needless_collect)] // bad clippy
    let fields_used_as_inputs = item
        .fields
        .iter()
        .filter_map(parse::Field::as_array)
        .flat_map(|array| array.count.iter_input_fields())
        .collect::<Vec<_>>();

    // The fields in the declaration of the struct.
    let mut field_decls = Vec::new();
    // the getters for those fields that have getters
    let mut getters = Vec::new();
    // just the names; we use these at the end to assemble the struct (shorthand initializer syntax)
    let mut used_field_names = Vec::new();
    // the code to intiailize each field. each block of code is expected to take the form,
    // `let (field_name, bytes) = $expr`, where 'bytes' is the whatever is left over
    // from the input bytes after validating this field.
    let mut field_inits = Vec::new();

    for field in &item.fields {
        if matches!(field, parse::Field::Array(arr) if arr.variable_size.is_some()) {
            continue;
        }

        let name = field.name();
        let span = name.span();

        field_decls.push(field.view_field_decl());
        used_field_names.push(field.name());

        if let Some(getter) = field.view_getter_fn() {
            getters.push(getter);
        }

        let field_init = field.view_init_expr();
        // if this field is used by another field, resolve it's current value
        let maybe_resolved_value = fields_used_as_inputs.contains(&name).then(|| {
            let resolved_ident = make_resolved_ident(name);
            quote_spanned!(span=> let #resolved_ident = #name.read().get();)
        });

        field_inits.push(quote! {
            #field_init
            #maybe_resolved_value
        });
    }

    quote! {
        #( #docs )*
        pub struct #name<'a> {
            #( #field_decls ),*
        }

        impl<'a> font_types::FontRead<'a> for #name<'a> {
            fn read(bytes: &'a [u8]) -> Option<Self> {
                #( #field_inits )*
                let _ = bytes;
                Some(#name {
                    #( #used_field_names, )*
                })
            }
        }

        impl<'a> #name<'a> {
            #( #getters )*
        }
    }
}

fn make_resolved_ident(ident: &syn::Ident) -> syn::Ident {
    quote::format_ident!("__resolved_{}", ident)
}
