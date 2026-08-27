//! codegen for generic group types

use proc_macro2::TokenStream;
use quote::quote;

use crate::parsing::GenericGroup;

pub(crate) fn generate(item: &GenericGroup) -> syn::Result<TokenStream> {
    let docs = &item.attrs.docs;
    let name = &item.name;
    let inner = &item.inner_type;

    let mut variant_decls = Vec::new();
    let mut read_match_arms = Vec::new();
    let mut of_unit_arms = Vec::new();
    for var in &item.variants {
        let var_name = &var.name;
        let type_id = &var.type_id;
        let typ = &var.typ;
        variant_decls.push(quote! { #var_name ( #inner <'a, #typ<'a>> ) });
        read_match_arms
            .push(quote! { #type_id => Ok(#name :: #var_name (FontRead::read(bytes)?)) });
        of_unit_arms.push(quote! { #name :: #var_name(inner) => inner.of_unit_type()  });
    }

    let first_var_name = &item.variants.first().unwrap().name;

    let of_unit_docs = &[
        " Return the inner table, removing the specific generics.",
        "",
        " This lets us return a single concrete type we can call methods on.",
    ];

    Ok(quote! {
        #( #docs)*
        pub enum #name <'a> {
            #( #variant_decls, )*
        }

        impl Default for #name<'_> {
            fn default() -> Self {
                Self::#first_var_name(Default::default())
            }
        }

        impl ReadArgs for #name<'_> {
            type Args = ();
        }

        impl<'a> FontRead<'a> for #name <'a> {
            fn read_with_args(bytes: FontData<'a>, _: ()) -> Result<Self, ReadError> {
                let discriminant = #inner::read_discriminant(bytes)?;
                match discriminant {
                    #( #read_match_arms, )*
                    other => Err(ReadError::InvalidFormat(other.into())),
                }
            }
        }

        impl<'a> #name <'a> {
            #[allow(dead_code)]
            #(  #[doc = #of_unit_docs] )*
            pub(crate) fn of_unit_type(&self) -> #inner<'a, ()> {
                match self {
                    #( #of_unit_arms, )*
                }
            }
        }

    })
}

pub(crate) fn generate_compile(
    item: &GenericGroup,
    parse_module: &syn::Path,
) -> syn::Result<TokenStream> {
    let docs = &item.attrs.docs;
    let name = &item.name;
    let inner = &item.inner_type;

    let mut variant_decls = Vec::new();
    let mut write_match_arms = Vec::new();
    let mut validate_match_arms = Vec::new();
    let mut from_obj_match_arms = Vec::new();
    let mut type_arms = Vec::new();
    let mut from_impls = Vec::new();
    let from_type = quote!(#parse_module :: #name);
    for var in &item.variants {
        let var_name = &var.name;
        let typ = &var.typ;

        variant_decls.push(quote! { #var_name ( #inner <#typ> ) });
        write_match_arms.push(quote! { Self :: #var_name (table) => table.write_into(writer)  });
        validate_match_arms.push(quote! { Self :: #var_name(table) => table.validate_impl(ctx) });
        from_obj_match_arms.push(
            quote! { #from_type :: #var_name(table) => Self :: #var_name(table.to_owned_obj(data)) },
        );
        type_arms.push(quote! { Self:: #var_name(table) => table.table_type()  });
        from_impls.push(quote! {
            impl From<#inner <#typ>> for #name {
                fn from(src: #inner <#typ>) -> #name {
                    #name :: #var_name ( src )
                }
            }
        });
    }
    let first_var_name = &item.variants.first().unwrap().name;

    Ok(quote! {
        #( #docs)*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum #name {
            #( #variant_decls, )*
        }

        impl Default for #name {
            fn default() -> Self {
                Self::#first_var_name(Default::default())
            }
        }

        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                match self {
                    #( #write_match_arms, )*
                }
            }

            fn table_type(&self) -> TableType {
                match self {
                    #( #type_arms, )*
                }
            }
        }

        impl Validate for #name {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                match self {
                    #( #validate_match_arms, )*
                }
            }
        }

        impl FromObjRef< #from_type :: <'_>> for #name {
            fn from_obj_ref(from: & #from_type :: <'_>, data: FontData) -> Self {
                match from {
                    #( #from_obj_match_arms, )*
                }
            }
        }

        impl FromTableRef< #from_type <'_>> for #name {}

        #( #from_impls )*

    })
}
