use fields::Field;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

mod fields;

#[proc_macro_derive(FontThing, attributes(font_thing))]
pub fn font_thing(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    derive_font_thing(input).unwrap_or_else(|err| err.to_compile_error().into())
    //todo!()
}

pub(crate) fn derive_font_thing(
    input: syn::DeriveInput,
) -> Result<proc_macro::TokenStream, syn::Error> {
    match &input.data {
        syn::Data::Struct(s) => derive_struct(&input, s),
        syn::Data::Enum(e) => derive_enum(&input, e),
        _ => Err(syn::Error::new(
            input.span(),
            "only structs supported for now",
        )),
    }
}

fn derive_struct(
    input: &syn::DeriveInput,
    item: &syn::DataStruct,
) -> Result<TokenStream, syn::Error> {
    let fields = item
        .fields
        .iter()
        .map(fields::Field::parse)
        .collect::<Result<Vec<_>, _>>()?;

    let ident = &input.ident;
    let ty_generics = get_generics(&input.generics)?;
    let trait_generics = ty_generics.clone().unwrap_or_else(|| quote!(<'font>));

    let offset_var = syn::Ident::new("__very_private_internal_offset", input.ident.span());
    let field_inits = fields
        .iter()
        .map(|field| init_field(field, &fields, &offset_var))
        .collect::<Result<Vec<_>, _>>()?;
    let names = fields.iter().map(|f| &f.name);
    let view_part = make_view(input, &fields, &ty_generics)?;
    let exact_sized_part = impl_exact_sized(input, &fields, &ty_generics)?;

    let decl = quote! {

        #[automatically_derived]
        impl #trait_generics ::toy_types::FontRead #trait_generics for #ident #ty_generics {
            fn read(blob: ::toy_types::Blob #trait_generics) -> Option<Self> {
                let mut #offset_var = 0;

                #( #field_inits )*

                Some(#ident {
                    #(#names),*
                })
            }
        }

        #view_part
        #exact_sized_part
    };
    Ok(decl.into())
}

fn derive_enum(input: &syn::DeriveInput, item: &syn::DataEnum) -> Result<TokenStream, syn::Error> {
    // ensure that a format type is specified:
    let format = get_enum_format(input)?;
    let format_values: Vec<_> = item
        .variants
        .iter()
        .map(get_variant_format)
        .collect::<Result<_, _>>()?;
    let variant_names = item.variants.iter().map(|variant| &variant.ident);

    let ident = &input.ident;
    let ty_generics = get_generics(&input.generics)?;
    let trait_generics = ty_generics.clone().unwrap_or_else(|| quote!(<'font>));
    let match_arms = format_values.iter().zip(variant_names).map(|(value, variant)| {
        quote!( #value => Some(Self::#variant(::toy_types::FontRead::read(blob)?)), )
    });

    let decl = quote! {
        #[automatically_derived]
        impl #trait_generics ::toy_types::FontRead #trait_generics for #ident #ty_generics {
            fn read(blob: ::toy_types::Blob #trait_generics) -> Option<Self> {
                let tag: #format = blob.read(0)?;
                match tag {
                    #(#match_arms)*

                        other => {
                            eprintln!("unknown enum variant {:?}", tag);
                            None
                        }
                }
            }
        }
    };

    Ok(decl.into())
}

fn get_enum_format(item: &syn::DeriveInput) -> Result<syn::Path, syn::Error> {
    let list = expect_meta_list(&item.attrs, "font_thing", item.ident.span())?;
    let first = list
        .nested
        .iter()
        .next()
        .ok_or_else(|| syn::Error::new(list.span(), "expected enum format specification"))?;
    match first {
        syn::NestedMeta::Meta(syn::Meta::List(list)) if list.path.is_ident("format") => {
            //dbg!(&list);
            if let Some(syn::NestedMeta::Meta(syn::Meta::Path(path))) = list.nested.iter().next() {
                return Ok(path.clone());
            }
        }
        _ => (),
    };
    Err(syn::Error::new(
        first.span(),
        "expected enum format type specification, like #[font_thing(format(uint16))]",
    ))
}

fn expect_meta_list(
    attrs: &[syn::Attribute],
    ident: &str,
    err_span: proc_macro2::Span,
) -> Result<syn::MetaList, syn::Error> {
    let item = attrs
        .iter()
        .find(|attr| attr.path.is_ident(ident))
        .ok_or_else(|| {
            syn::Error::new(err_span, format!("expected #[{}()] attribute list", ident))
        })?;

    match item.parse_meta()? {
        syn::Meta::List(list) => Ok(list.clone()),
        _ => Err(syn::Error::new(item.span(), "expected attribute list")),
    }
}

fn get_variant_format(item: &syn::Variant) -> Result<syn::Lit, syn::Error> {
    let attrs = expect_meta_list(&item.attrs, "font_thing", item.ident.span())?;
    let first = attrs
        .nested
        .iter()
        .next()
        .ok_or_else(|| syn::Error::new(attrs.span(), "expected enum format specification"))?;

    match first {
        syn::NestedMeta::Meta(syn::Meta::NameValue(val)) if val.path.is_ident("format") => {
            return Ok(val.lit.clone());
            //if let Some(syn::NestedMeta::Meta(syn::Meta::Path(path))) = list.nested.iter().next() {
            //return Ok(path.clone());
            //}
        }
        _ => (),
    };
    Err(syn::Error::new(
        first.span(),
        "expected enum format type specification, like #[font_thing(format(uint16))]",
    ))
}

fn make_view(
    input: &syn::DeriveInput,
    fields: &[Field],
    ty_generics: &Option<proc_macro2::TokenStream>,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ident = &input.ident;
    let view_ident = syn::Ident::new(&format!("{}DerivedView", &input.ident), input.ident.span());
    let mut field_offset = quote!(0_usize);
    let getters = fields
        .iter()
        .map(|x| field_getter(x, fields, &mut field_offset));
    let trait_generics = ty_generics.clone().unwrap_or_else(|| quote!(<'font>));

    Ok(quote! {
        pub struct #view_ident #trait_generics(::toy_types::Blob #trait_generics);

        #[automatically_derived]
        #[allow(clippy::len_without_is_empty)]
        impl #trait_generics #view_ident #trait_generics {
            #( #getters )*

        }

        #[automatically_derived]
        impl #trait_generics ::toy_types::FontRead #trait_generics for #view_ident #trait_generics {
            fn read(blob: ::toy_types::Blob #trait_generics) -> Option<Self> {
                Some(Self(blob))
            }
        }

        impl #trait_generics ::toy_types::FontThing #trait_generics for #ident #ty_generics {
            type View = #view_ident #trait_generics;
        }
    })
}

fn impl_exact_sized(
    input: &syn::DeriveInput,
    fields: &[Field],
    ty_generics: &Option<proc_macro2::TokenStream>,
) -> Result<Option<proc_macro2::TokenStream>, syn::Error> {
    // we only impl this if all fields are scalars
    if fields.iter().any(|fld| fld.attrs.is_some()) {
        return Ok(None);
    }

    let sizes = fields.iter().map(|fld| {
        let ty = &fld.ty;
        quote!(<#ty as ::toy_types::ExactSized>::SIZE)
    });

    let ident = &input.ident;
    Ok(Some(quote! {
        impl #ty_generics ::toy_types::ExactSized for #ident {
            const SIZE: usize = #( #sizes )+*;
        }
    }))
}

/// Check that generic arguments are acceptable
///
/// They are acceptable if they are empty, or contain a single lifetime.
///
/// We return tokens (possibly empty) to append to impl blocks for the type.
/// As in: if the type has a declared lifetime, we need to have that lifetime
/// match the lifetime in the traits we're implementing.
fn get_generics(generics: &syn::Generics) -> Result<Option<proc_macro2::TokenStream>, syn::Error> {
    if generics.type_params().count() + generics.const_params().count() > 0 {
        return Err(syn::Error::new(
            generics.span(),
            "generics are not allowed in font tables",
        ));
    }
    if let Some(lifetime) = generics.lifetimes().nth(1) {
        return Err(syn::Error::new(
            lifetime.span(),
            "tables can contain at most a single lifetime",
        ));
    }

    Ok(generics.lifetimes().next().map(|gen| {
        let lifetime = &gen.lifetime;
        quote! {<#lifetime>}
    }))
}

fn init_field(
    field: &Field,
    all: &[Field],
    offset_var: &syn::Ident,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &field.name;
    let type_ = &field.ty;
    let is_last = field.name == all.last().unwrap().name;

    match field.attrs.as_ref() {
        None => Ok(quote! {
            let #name = {
                let temp: #type_ = blob.read(#offset_var)?;
                let len = <#type_ as ::toy_types::ExactSized>::SIZE;
                #offset_var += usize::from(len);
                temp
            };
        }),
        Some(attrs) if attrs.data.is_some() => {
            //TODO: validate these, make sure data is last item?
            Ok(quote!(let #name = blob;))
        }

        Some(attrs) => {
            let count = attrs
                .count_fn
                .as_ref()
                .map(|count_fn| {
                    let fn_ = &count_fn.fn_;
                    let args = count_fn.args.iter();
                    quote!(#fn_( #(#args),* ))
                })
                .or_else(|| attrs.count.as_ref().map(|ident| quote!(#ident)));
            let decl = if let Some(count) = count {
                quote!(<#type_>::new(blob.clone(), #offset_var.into(), #count.into()))
            } else if attrs.count_all.is_some() {
                quote!(<#type_>::new_no_len(blob.clone(), #offset_var.into()))
            } else {
                return Err(syn::Error::new(
                    field.name.span(),
                    "expected 'all' or 'count' attribute",
                ));
            };

            // the last item is allowed to have unknown length, so doesn't always
            // have a len() method; we just don't call len() if we're the last item.
            let update_offset = (!is_last).then(|| quote!(#offset_var += #name.data_len()));
            Ok(quote! {
                let #name = #decl?;
                #update_offset;
            })
        }
    }
}

/// offset_val is something we accumulate while building the fields; each field
/// appends its length to the input value before returning
fn field_getter(
    field: &Field,
    _all: &[Field],
    offset_val: &mut proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let type_ = &field.ty;
    let name = &field.name;
    let vis = &field.vis;
    let this_off = offset_val.clone();

    let getter_body = match &field.attrs {
        None => {
            let this_len = quote! { <#type_ as ::toy_types::ExactSized>::SIZE };
            *offset_val = quote! { #offset_val + #this_len };

            quote! {
                    //FIXME: this should assume that length has been checked,
                    //(and we should be checking length in the constructor)
                    //assert this somehow, and then use unsafe
                    self.0.read(#this_off)
            }
        }
        //FIXME: figure out how we hold on to data
        Some(attrs) if attrs.data.is_some() => return Default::default(),
        Some(attrs) => {
            let get_count_fn = attrs
                .count_fn
                .as_ref()
                .map(|count_fn| {
                    let fn_ = &count_fn.fn_;
                    let args = count_fn.args.iter();
                    quote!(#fn_( #(self.#args()?),* ))
                })
                .or_else(|| attrs.count.as_ref().map(|ident| quote!(self.#ident()?)));

            if let Some(count_fn) = &get_count_fn {
                *offset_val = quote!(#offset_val + usize::from(#count_fn));
            }

            if let Some(count) = &get_count_fn {
                quote!(<#type_>::new(self.0.clone(), #this_off, #count.into()))
            } else if attrs.count_all.is_some() {
                quote!(<#type_>::new_no_len(self.0.clone(), #this_off))
            } else {
                unreachable!("attributes are validated when fields are made?");
            }
        }
    };
    quote! {
        #vis fn #name(&self) -> Option<#type_> {
            #getter_body
        }
    }
}
