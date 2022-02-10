use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

mod parse;

#[proc_macro]
pub fn tables(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as parse::Item);
    let name = &input.name;
    let field_names = input.fields.iter().map(|field| &field.name);
    let field_types = input
        .fields
        .iter()
        .map(|field| field.concrete_type_tokens());

    quote! {
        pub struct #name {
            #( pub #field_names: #field_types, )*
        }
    }
    .into()
}
