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
            //Item::Format(item) => todo!(),
            Item::RawEnum(item) => flags_enums::generate_raw_enum(&item),
            Item::Flags(item) => flags_enums::generate_flags(&item),
            _ => Default::default(),
        };
        code.push(item_code);
    }

    //let use_stmts = &items.use_stmts;
    Ok(quote! {
        //#(#use_stmts)*
        #[allow(unused_imports)]
        use crate::parse_prelude::*;
        #(#code)*
    })
}
