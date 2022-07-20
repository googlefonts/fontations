mod fields;
mod flags_enums;
mod parsing;
mod record;
mod table;

use parsing::{Item, Items};
use quote::quote;

pub fn generate_parse_module(code: &str) -> Result<proc_macro2::TokenStream, syn::Error> {
    let items: Items = syn::parse_str(code)?;
    let mut code = Vec::new();
    for item in &items.items {
        let item_code = match item {
            Item::Record(item) => record::generate(item)?,
            Item::Table(item) => table::generate(item)?,
            Item::Format(item) => table::generate_format_group(item)?,
            Item::RawEnum(item) => flags_enums::generate_raw_enum(&item),
            Item::Flags(item) => flags_enums::generate_flags(&item),
        };
        code.push(item_code);
    }

    Ok(quote! {
        #[allow(unused_imports)]
        use crate::parse_prelude::*;
        #(#code)*
    })
}

pub fn generate_compile_module(code: &str) -> Result<proc_macro2::TokenStream, syn::Error> {
    let items: Items = syn::parse_str(code)?;

    let code = items
        .items
        .iter()
        .map(|item| match item {
            Item::Record(item) => record::generate_compile(&item),
            Item::Table(item) => table::generate_compile(&item),
            Item::Format(item) => table::generate_format_compile(&item),
            _ => Ok(Default::default()),
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(quote! {
        #[allow(unused_imports)]
        use crate::compile_prelude::*;

        #( #code )*
    })
}
