//! codegen for table objects

use quote::quote;

use super::parsing::Table;

pub(crate) fn generate(item: &Table) -> syn::Result<proc_macro2::TokenStream> {
    let marker_name = &item.name;
    let shape_name = item.shape_name();
    let shape_byte_range_fns = item.iter_shape_byte_fns();
    let shape_fields = item.iter_shape_fields();

    let field_validation_stmts = item.iter_field_validation_stmts();
    let shape_field_names = item.iter_shape_field_names();

    let table_ref_getters = item.iter_table_ref_getters();

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

        impl TableInfo for #marker_name {
            type Info = #shape_name;

            fn parse<'a>(data: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
                let mut cursor = data.cursor();
                #( #field_validation_stmts )*
                cursor.finish( #shape_name {
                    #( #shape_field_names, )*
                })
            }
        }

        impl<'a> TableRef<'a, #marker_name> {

            #( #table_ref_getters )*

        }
    })
}
