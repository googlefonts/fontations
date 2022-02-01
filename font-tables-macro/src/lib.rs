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
    let ty_generics = get_generics(&input.generics)?;
    let trait_generics = ty_generics.clone().unwrap_or_else(|| quote!(<'font>));

    let offset_var = syn::Ident::new("__very_private_internal_offset", input.ident.span());
    let field_inits = fields
        .iter()
        .map(|field| init_field(field, &fields, &offset_var));
    let names = fields.iter().map(|f| &f.name);
    let view_part = make_view(input, &fields, &ty_generics)?;
    let exact_sized_part = impl_exact_sized(input, &fields, &ty_generics)?;

    let decl = quote! {
        impl #trait_generics ::toy_types::FontRead #trait_generics for #ident #ty_generics {
            fn read(blob: ::toy_types::Blob #trait_generics) -> Option<Self> {
                let mut #offset_var = 0;

                #( #field_inits )*

                Some(#ident {
                    #(#names),*
                })
            }
        }

        #view_part
        #exact_sized_part
    };
    Ok(decl.into())
}

fn make_view(
    input: &syn::DeriveInput,
    fields: &[Field],
    ty_generics: &Option<proc_macro2::TokenStream>,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ident = &input.ident;
    let view_ident = syn::Ident::new(&format!("{}DerivedView", &input.ident), input.ident.span());
    let mut field_offset = quote!(0_usize);
    let getters = fields
        .iter()
        .map(|x| field_getter(x, fields, &mut field_offset));
    let trait_generics = ty_generics.clone().unwrap_or_else(|| quote!(<'font>));

    Ok(quote! {
        pub struct #view_ident #trait_generics(::toy_types::Blob #trait_generics);

        impl #trait_generics #view_ident #trait_generics {
            #( #getters )*

        }

        impl #trait_generics ::toy_types::FontRead #trait_generics for #view_ident #trait_generics {
            fn read(blob: ::toy_types::Blob #trait_generics) -> Option<Self> {
                Some(Self(blob))
            }
        }

        impl #trait_generics ::toy_types::FontThing #trait_generics for #ident #ty_generics {
            type View = #view_ident #trait_generics;
        }
    })
}

fn impl_exact_sized(
    input: &syn::DeriveInput,
    fields: &[Field],
    ty_generics: &Option<proc_macro2::TokenStream>,
) -> Result<Option<proc_macro2::TokenStream>, syn::Error> {
    // we only impl this if all fields are scalars
    if fields.iter().any(|fld| fld.attrs.is_some()) {
        return Ok(None);
    }

    let sizes = fields.iter().map(|fld| {
        let ty = &fld.ty;
        quote!(<#ty as ::toy_types::ExactSized>::SIZE)
    });

    let ident = &input.ident;
    Ok(Some(quote! {
        impl #ty_generics ::toy_types::ExactSized for #ident {
            const SIZE: usize = #( #sizes )+*;
        }
    }))
}

/// Check that generic arguments are acceptable
///
/// They are acceptable if they are empty, or contain a single lifetime.
///
/// We return tokens (possibly empty) to append to impl blocks for the type.
/// As in: if the type has a declared lifetime, we need to have that lifetime
/// match the lifetime in the traits we're implementing.
fn get_generics(generics: &syn::Generics) -> Result<Option<proc_macro2::TokenStream>, syn::Error> {
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

    Ok(generics.lifetimes().next().map(|gen| {
        let lifetime = &gen.lifetime;
        quote! {<#lifetime>}
    }))
}

fn init_field(field: &Field, all: &[Field], offset_var: &syn::Ident) -> proc_macro2::TokenStream {
    let name = &field.name;
    let type_ = &field.ty;
    let is_last = field.name == all.last().unwrap().name;

    match field.attrs.as_ref() {
        None => {
            quote! {
                let #name = {
                    let temp: #type_ = blob.read(#offset_var)?;
                    let len = <#type_ as ::toy_types::ExactSized>::SIZE;
                    #offset_var += usize::from(len);
                    temp
                };
            }
        }
        Some(attrs) if attrs.data.is_some() => {
            //TODO: validate these, make sure data is last item?
            quote!(let #name = blob;)
        }

        Some(attrs) => {
            let count = attrs
                .count_fn
                .as_ref()
                .map(|count_fn| {
                    let fn_ = &count_fn.fn_;
                    let args = count_fn.args.iter();
                    quote!(#fn_( #(#args),* ))
                })
                .or_else(|| attrs.count.as_ref().map(|ident| quote!(#ident)))
                .unwrap_or_else(|| {
                    quote!(compile_error!(
                        "TODO: validate attributes before generating fields"
                    ))
                });
            // the last item is allowed to have unknown length, so doesn't always
            // have a len() method; we just don't call len() if we're the last item.
            let update_offset = (!is_last).then(|| quote!(#offset_var += #name.len()));
            quote! {
                let #name = <#type_>::new(blob.clone(), #offset_var.into(), #count.into())?;
                #update_offset;
            }
        }
    }
}

/// offset_val is something we accumulate while building the fields; each field
/// appends its length to the input value before returning
fn field_getter(
    field: &Field,
    _all: &[Field],
    offset_val: &mut proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let type_ = &field.ty;
    let name = &field.name;
    let vis = &field.vis;
    let this_off = offset_val.clone();

    let getter_body = match &field.attrs {
        None => {
            let this_len = quote! { <#type_ as ::toy_types::ExactSized>::SIZE };
            *offset_val = quote! { #offset_val + #this_len };

            quote! {
                    //FIXME: this should assume that length has been checked,
                    //(and we should be checking length in the constructor)
                    //assert this somehow, and then use unsafe
                    self.0.read(#this_off)
            }
        }
        //FIXME: figure out how we hold on to data
        Some(attrs) if attrs.data.is_some() => return Default::default(),
        Some(attrs) => {
            let get_count_fn = attrs
                .count_fn
                .as_ref()
                .map(|count_fn| {
                    let fn_ = &count_fn.fn_;
                    let args = count_fn.args.iter();
                    quote!(#fn_( #(self.#args()?),* ))
                })
                .or_else(|| attrs.count.as_ref().map(|ident| quote!(self.#ident()?)))
                .unwrap_or_else(|| {
                    quote!(compile_error!(
                        "TODO: validate attributes before generating fields"
                    ))
                });

                *offset_val = quote!(#offset_val + usize::from(#get_count_fn));
                quote! {
                    let count = #get_count_fn;
                    <#type_>::new(self.0.clone(), #this_off, count.into())
                }
        }
    };
    quote! {
        #vis fn #name(&self) -> Option<#type_> {
            #getter_body
        }
    }
}
