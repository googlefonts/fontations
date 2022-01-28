use fields::Field;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

mod fields;

#[proc_macro_derive(FontThing, attributes(font_thing))]
pub fn font_thing(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    derive_font_thing(input).unwrap_or_else(|err| err.to_compile_error().into())
    //todo!()
}

pub(crate) fn derive_font_thing(
    input: syn::DeriveInput,
) -> Result<proc_macro::TokenStream, syn::Error> {
    match &input.data {
        syn::Data::Struct(s) => derive_struct(&input, s),
        _ => Err(syn::Error::new(
            input.span(),
            "only structs supported for now",
        )),
    }
}

fn derive_struct(
    input: &syn::DeriveInput,
    item: &syn::DataStruct,
) -> Result<TokenStream, syn::Error> {
    let fields = item
        .fields
        .iter()
        .map(fields::Field::parse)
        .collect::<Result<Vec<_>, _>>()?;
    // get a lifetime if needed
    let ident = &input.ident;
    let _lifetime = generate_lifetime(&input.generics);
    // make the init code that finds each field's position.
    //let field_inits = init_fields(&fields);
    let offset_var = syn::Ident::new("__very_private_internal_offset", input.ident.span());
    let field_inits = fields
        .iter()
        .map(|field| init_field(field, &fields, &offset_var));
    let names = fields.iter().map(|f| &f.name);

    let decl = quote! {
        impl<'font> ::font_types::FromBytes<'font> for #ident {
            fn from_bytes(bytes: &'font [u8]) -> Option<Self> {
                let mut #offset_var = 0;

                #( #field_inits )*

                Some(#ident {
                    #(#names),*
                })
            }
        }
    };
    Ok(decl.into())
    //TODO: error if any generics etc are present
    //input.attrs
    //todo!()
}

fn generate_lifetime(_generics: &syn::Generics) -> Result<proc_macro2::TokenStream, syn::Error> {
    Ok(quote!())
}

fn init_field(field: &Field, _all: &[Field], offset_var: &syn::Ident) -> proc_macro2::TokenStream {
    let name = &field.name;
    let type_ = &field.ty;
    if field.attrs.is_none() {
        quote! {
            let #name = {
                let len = <#type_ as ::font_types::ExactSized>::SIZE;
                let range = #offset_var..#offset_var + len;
                let temp: #type_ = ::font_types::FromBeBytes::read(bytes.get(range)?.try_into().ok()?).ok()?;
                #offset_var += usize::from(len);
                temp
            };
        }.into()
    } else {
        quote! {
            let #name = Default::default();
        }
        .into()
    }
}

//pub trait FromBytes<'a>: Sized {
///// If this type has a single known size, it is declared here.
/////
///// If this is declared, it is always used(?)
//const FIXED_SIZE: Option<usize>;

//fn from_bytes(bytes: &'a [u8]) -> Option<(Self)>;
//fn byte_len(&self) -> usize;
//}

//pub trait FontThing<'a>: FromBytes<'a> {
//type View: FromBytes<'a>;
//const SIZE_HINT: Option<usize>;
//}

#[proc_macro]
pub fn font_tables(input: TokenStream) -> TokenStream {
    //let span = input.();
    let input = proc_macro2::TokenStream::from(input);
    let strings = input
        .into_iter()
        .map(|item| item_type(&item))
        .collect::<Vec<_>>();
    dbg!(strings);
    //let err = syn::Error::new(input.span(), strings.join(", "));
    //for item in input {

    //}
    //let _ = input;
    //let input = parse_macro_input!(input);

    unimplemented!()
}

//fn generate_item(input: &proc_macro2::TokenTree) -> Result<proc_macro2::TokenStream, syn::Error> {
//Err(syn::Error::new_spanned(input, "idk man"))

//}

fn item_type(tree: &proc_macro2::TokenTree) -> String {
    match tree {
        proc_macro2::TokenTree::Group(_g) => format!("Group"),
        proc_macro2::TokenTree::Ident(i) => format!("ident {}", i),
        proc_macro2::TokenTree::Punct(i) => format!("{}", i),
        proc_macro2::TokenTree::Literal(i) => format!("L{}", i),
    }
}
