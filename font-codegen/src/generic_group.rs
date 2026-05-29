//! codegen for generic group types

use proc_macro2::TokenStream;
use quote::quote;

use crate::parsing::{GenericGroup, Items};

pub(crate) fn generate(item: &GenericGroup, items: &Items) -> syn::Result<TokenStream> {
    let docs = &item.attrs.docs;
    let name = &item.name;
    let inner = &item.inner_type;

    let mut variant_decls = Vec::new();
    let mut read_match_arms = Vec::new();
    let mut dyn_inner_arms = Vec::new();
    let mut of_unit_arms = Vec::new();
    for var in &item.variants {
        let var_name = &var.name;
        let type_id = &var.type_id;
        let typ = &var.typ;
        variant_decls.push(quote! { #var_name ( #inner <'a, #typ<'a>> ) });
        read_match_arms
            .push(quote! { #type_id => Ok(#name :: #var_name (FontRead::read(bytes)?)) });
        dyn_inner_arms.push(quote! { #name :: #var_name(table) => table });
        of_unit_arms.push(quote! { #name :: #var_name(inner) => inner.of_unit_type()  });
    }

    let first_var_name = &item.variants.first().unwrap().name;

    let of_unit_docs = &[
        " Return the inner table, removing the specific generics.",
        "",
        " This lets us return a single concrete type we can call methods on.",
    ];

    let sanitize = generate_sanitize(item, items);

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

        #sanitize

        #[cfg(feature = "experimental_traverse")]
        impl<'a> #name <'a> {
            fn dyn_inner(&self) -> &(dyn SomeTable<'a> + 'a) {
                match self {
                    #( #dyn_inner_arms, )*
                }
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> SomeTable<'a> for #name <'a> {

            fn get_field(&self, idx: usize) -> Option<Field<'a>> {
                self.dyn_inner().get_field(idx)
            }

            fn type_name(&self) -> &str {
                self.dyn_inner().type_name()
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl std::fmt::Debug for #name<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.dyn_inner().fmt(f)
            }
        }
    })
}

fn generate_sanitize(item: &GenericGroup, items: &Items) -> Option<TokenStream> {
    if !items.sanitize {
        return None;
    }
    let name = &item.name;
    let inner = &item.inner_type;

    let match_arms: Vec<_> = item
        .variants
        .iter()
        .map(|var| {
            let type_id = &var.type_id;
            let typ = &var.typ;
            quote!(#type_id => #inner::<#typ>::sanitize(ctx, _args),)
        })
        .collect();

    // `read_fast` dispatches on the discriminant like `sanitize`, constructing the
    // matched variant via its inner table's `read_fast` (no validation).
    let read_fast_arms: Vec<_> = item
        .variants
        .iter()
        .map(|var| {
            let type_id = &var.type_id;
            let var_name = &var.name;
            let typ = &var.typ;
            quote!(Ok(#type_id) => #name::#var_name(#inner::<#typ>::read_fast(data, ())),)
        })
        .collect();

    // soundness: a generic group must have at least one variant.
    let _ = item.variants.first().expect("generic group needs variants");

    Some(quote! {
        impl<'a> Sanitize<'a> for #name<'a> {
            fn sanitize(ctx: &mut SanitizeContext<'a, '_>, _args: ()) -> Result<(), ReadError> {
                let discriminant = #inner::read_discriminant(ctx.data())?;
                match discriminant {
                    #( #match_arms )*
                    other => Err(ReadError::InvalidFormat(other as _)),
                }
            }

            fn read_fast(data: FontData<'a>, _args: ()) -> Self {
                // An unreadable or unknown discriminant routes to the default
                // table rather than re-reading `data` as the first variant.
                match #inner::read_discriminant(data) {
                    #( #read_fast_arms )*
                    _ => #name::default(),
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
