use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

mod parse;

#[proc_macro]
pub fn tables(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as parse::Items);
    let code = input.iter().map(generate_item_code);
    quote! {
        #(#code)*
    }
    .into()
}

fn generate_item_code(item: &parse::Item) -> proc_macro2::TokenStream {
    if item.fields.iter().all(|x| x.is_scalar()) {
        generate_zerocopy_impls(item)
    } else {
        generate_view_impls(item)
    }
}

fn generate_zerocopy_impls(item: &parse::Item) -> proc_macro2::TokenStream {
    assert!(!item.lifetime);
    let name = &item.name;
    let field_names = item
        .fields
        .iter()
        .map(|field| &field.as_scalar().unwrap().name);
    let field_types = item.fields.iter().map(parse::Field::concrete_type_tokens);

    quote! {
        #[derive(Clone, Copy, Debug, zerocopy::FromBytes, zerocopy::Unaligned)]
        #[repr(C)]
        pub struct #name {
            #( pub #field_names: #field_types, )*
        }

        // and now I want getters. but not for everything? also they return... a different concrete
        // type? or... do I want this, in the zero-copy version?
    }
}

fn generate_view_impls(item: &parse::Item) -> proc_macro2::TokenStream {
    // scalars only get getters? that makes 'count' and friends complicated...
    // we can at least have a 'new' method that does a reasonable job of bounds checking,
    // but then we're going to be unsafing all over. that's also maybe okay though

    let name = &item.name;
    let checkable_len = item.checkable_len();

    //TODO: getters

    quote! {
        pub struct #name<'a>(&'a [u8]);

        impl<'a> #name<'a> {
            pub fn new(bytes: &'a [u8]) -> Option<Self> {
                if bytes.len() < #checkable_len {
                    return None;
                }
                Some(Self(bytes))
            }
        }
    }
}
