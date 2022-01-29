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

    let ident = &input.ident;
    let generics = get_generics(&input.generics)?;

    let offset_var = syn::Ident::new("__very_private_internal_offset", input.ident.span());
    let field_inits = fields
        .iter()
        .map(|field| init_field(field, &fields, &offset_var));
    let names = fields.iter().map(|f| &f.name);
    let view_part = make_view(input, &fields, &generics)?;

    let decl = quote! {
        impl<'font> ::toy_types::FontRead<'font> for #ident #generics {
            fn read(blob: ::toy_types::Blob<'font>) -> Option<Self> {
                let mut #offset_var = 0;

                #( #field_inits )*

                Some(#ident {
                    #(#names),*
                })
            }
        }

        #view_part
    };
    Ok(decl.into())
}

fn make_view(
    input: &syn::DeriveInput,
    fields: &[Field],
    generics: &proc_macro2::TokenStream,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ident = &input.ident;
    let view_ident = syn::Ident::new(&format!("{}DerivedView", &input.ident), input.ident.span());
    let getters = fields.iter().map(|x| field_getter(x, fields));

    Ok(quote! {
        pub struct #view_ident<'font>(::toy_types::Blob<'font>);

        impl<'font> #view_ident<'font> {
            #( #getters )*

        }

        impl<'font> ::toy_types::FontRead<'font> for #view_ident<'font> {
            fn read(blob: ::toy_types::Blob<'font>) -> Option<Self> {
                Some(Self(blob))
            }
        }

        impl<'font> ::toy_types::FontThing<'font> for #ident #generics {
            type View = #view_ident<'font>;
        }
    })
}

/// Check that generic arguments are acceptable
///
/// They are acceptable if they are empty, or contain a single lifetime.
///
/// We return tokens (possibly empty) to append to impl blocks for the type.
/// As in: if the type has a declared lifetime, we need to have that lifetime
/// match the lifetime in the traits we're implementing.
fn get_generics(generics: &syn::Generics) -> Result<proc_macro2::TokenStream, syn::Error> {
    if generics.type_params().count() + generics.const_params().count() > 0 {
        return Err(syn::Error::new(
            generics.span(),
            "generics are not allowed in font tables",
        ));
    }
    if let Some(lifetime) = generics.lifetimes().nth(1) {
        return Err(syn::Error::new(
            lifetime.span(),
            "tables can contain at most a single lifetime",
        ));
    }

    Ok(generics
        .lifetimes()
        .next()
        .is_some()
        .then(|| quote!(<'font>))
        .unwrap_or_default())
}

fn init_field(field: &Field, _all: &[Field], offset_var: &syn::Ident) -> proc_macro2::TokenStream {
    let name = &field.name;
    let type_ = &field.ty;
    if field.attrs.is_none() {
        quote! {
            let #name = {
                let temp: #type_ = blob.read(#offset_var)?;
                let len = <#type_ as ::toy_types::ExactSized>::SIZE;
                #offset_var += usize::from(len);
                temp
            };
        }
    } else {
        quote! {
            let #name = Default::default();
        }
    }
}

fn field_getter(field: &Field, all: &[Field]) -> proc_macro2::TokenStream {
    let type_ = &field.ty;
    let name = &field.name;

    if field.attrs.is_none() {
        let field_pos = all.iter().position(|i| i.name == field.name).unwrap();
        let init_off = if field_pos == 0 {
            quote! {
                let offset = 0_usize;
            }
        } else {
            let init_off = all.iter().take_while(|x| x.name != field.name).map(|x| {
                let t = &x.ty;
                quote! {
                    <#t as ::toy_types::ExactSized>::SIZE
                }
            });
            quote! {
                let offset = #( #init_off )+*;
            }
        };

        quote! {
            pub fn #name(&self) -> Option<#type_> {
                //FIXME: this should assume that length has been checked,
                //(and we should be checking length in the constructor)
                //assert this somehow, and then use unsafe
                #init_off
                self.0.read(offset)
            }
        }
    } else {
        //TODO: generate code for non-scalar fields
        quote!()
    }
}
