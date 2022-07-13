//! codegen for record objects

use quote::quote;

use super::parsing::{Field, Record};

pub(crate) fn generate(item: &Record) -> syn::Result<proc_macro2::TokenStream> {
    let name = &item.name;
    let field_names = item.fields.iter().map(|fld| &fld.name);
    let field_types = item.fields.iter().map(Field::type_for_record);
    let field_docs = item.fields.iter().map(|fld| {
        let docs = &fld.attrs.docs;
        quote!( #( #docs )* )
    });

    Ok(quote! {
        #[derive(Clone, Debug)]
        #[repr(C)]
        #[repr(packed)]
        pub struct #name {
            #( #field_docs pub #field_names: #field_types, )*
        }
    })
}
