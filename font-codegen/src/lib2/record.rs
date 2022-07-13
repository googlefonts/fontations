//! codegen for record objects

use super::parsing::Record;

pub(crate) fn generate(item: &Record) -> syn::Result<proc_macro2::TokenStream> {
    let name = &item.name;
    let field_names = item.fields.iter().map(|fld| &fld.name);
    Ok(Default::default())
    //let field_docs = item.fields.iter().map(|fld| &fld.)
}
