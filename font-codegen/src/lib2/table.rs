//! codegen for table objects

use quote::quote;

use super::parsing::Table;

pub(crate) fn generate(item: &Table) -> syn::Result<proc_macro2::TokenStream> {
    let marker_name = &item.name;
    let shape_name = item.shape_name();
    let shape_byte_range_fns = item.iter_shape_byte_fns();
    let shape_fields = item.iter_shape_fields();

    Ok(quote! {
        #[derive(Debug, Clone, Copy)]
        pub struct #marker_name;

        #[derive(Debug, Clone, Copy)]
        pub struct #shape_name {
            #( #shape_fields )*
        }

        impl #shape_name {
            #( #shape_byte_range_fns )*
        }
    })
}
