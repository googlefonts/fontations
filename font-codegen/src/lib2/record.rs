//! codegen for record objects

use quote::quote;

use super::parsing::{Field, Record};

pub(crate) fn generate(item: &Record) -> syn::Result<proc_macro2::TokenStream> {
    if item.attrs.skip_parse.is_some() {
        return Ok(Default::default());
    }

    let name = &item.name;
    let docs = &item.attrs.docs;
    let field_names = item.fields.iter().map(|fld| &fld.name).collect::<Vec<_>>();
    let field_types = item
        .fields
        .iter()
        .map(Field::type_for_record)
        .collect::<Vec<_>>();
    let field_docs = item.fields.iter().map(|fld| {
        let docs = &fld.attrs.docs;
        quote!( #( #docs )* )
    });
    let inner_types = item.fields.iter().map(|fld| fld.getter_return_type());

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug)]
        #[repr(C)]
        #[repr(packed)]
        pub struct #name {
            #( #field_docs pub #field_names: #field_types, )*
        }

        impl FixedSized for #name {
            const RAW_BYTE_LEN: usize = #( #inner_types::RAW_BYTE_LEN )+*;
        }
    })
}
