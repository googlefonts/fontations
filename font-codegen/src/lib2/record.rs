//! codegen for record objects

use quote::quote;

use super::parsing::{Field, Record};

pub(crate) fn generate(item: &Record) -> syn::Result<proc_macro2::TokenStream> {
    let name = &item.name;
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

    Ok(quote! {
        #[derive(Clone, Debug)]
        #[repr(C)]
        #[repr(packed)]
        pub struct #name {
            #( #field_docs pub #field_names: #field_types, )*
        }

        impl ReadScalar for #name {
            const RAW_BYTE_LEN: usize = #( std::mem::size_of::<#field_types>() )+*;
            fn read(bytes: &[u8]) -> Option<Self> {
                todo!()
            }
        }
    })
}
