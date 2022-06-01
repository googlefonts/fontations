use quote::quote;

use crate::parse;

pub fn generate_compile_module(
    parsed: &parse::Items,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let items = parsed
        .items
        .iter()
        .flat_map(|item| match item {
            parse::Item::Single(item) => Some(generate_single_item(item)),
            parse::Item::Group(item) => Some(generate_group(item)),
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;

    let use_paths = parsed.use_stmts.iter().map(|stmt| stmt.compile_use_stmt());
    Ok(quote! {
        #[cfg(feature = "compile")]
        pub mod compile {
            use crate::compile::*;
            #(use #use_paths;)*

            #(#items)*
        }
    })
}

pub fn generate_single_item(
    item: &parse::SingleItem,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &item.name;

    let mut field_decls = Vec::new();

    for field in &item.fields {
        if field.is_computed() {
            continue;
        }
        let name = field.name();
        let typ = field.compile_type();
        field_decls.push(quote!(pub #name: #typ));
    }

    Ok(quote! {
        #[derive(Debug, Default)]
        pub struct #name {
            #(#field_decls,)*
        }

        impl #name {
            pub fn new() -> Self {
                Default::default()
            }
        }
    })
}

pub fn generate_group(group: &parse::ItemGroup) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &group.name;
    let variants = group.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = &variant.typ;
        quote!(#name(#typ))
    });

    let first_variant = &group
        .variants
        .iter()
        .next()
        .ok_or_else(|| syn::Error::new(name.span(), "empty enums are not allowed"))?
        .name;

    Ok(quote! {
        #[derive(Debug)]
        pub enum #name {
            #(#variants),*
        }

        impl Default for #name {
            fn default() -> Self {
                Self::#first_variant(Default::default())
            }
        }
    })
}
