use quote::quote;

use crate::parse;

pub fn generate_compile_module(
    parsed: &parse::Items,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let items = parsed
        .items
        .iter()
        .flat_map(|item| match item {
            parse::Item::Single(item) => Some(generate_single_item(item)),
            parse::Item::Group(item) => Some(generate_group(item)),
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;

    let use_paths = parsed.use_stmts.iter().map(|stmt| stmt.compile_use_stmt());
    Ok(quote! {
        #[cfg(feature = "compile")]
        pub mod compile {
            use crate::compile::*;
            use font_types::{Offset as _, OffsetHost as _};
            #(use #use_paths;)*

            #(#items)*
        }
    })
}

fn generate_single_item(item: &parse::SingleItem) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &item.name;

    let mut field_decls = Vec::new();

    for field in &item.fields {
        if field.is_computed() {
            continue;
        }
        let name = field.name();
        let typ = field.compile_type();
        field_decls.push(quote!(pub #name: #typ));
    }

    let impl_from_obj = item
        .skip_from_obj
        .is_none()
        .then(|| item_from_obj(item))
        .transpose()?;

    Ok(quote! {
        #[derive(Debug, Default)]
        pub struct #name {
            #(#field_decls,)*
        }

        #impl_from_obj

        impl #name {
            pub fn new() -> Self {
                Default::default()
            }
        }
    })
}

fn item_from_obj(item: &parse::SingleItem) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &item.name;
    let lifetime = item.lifetime.is_some().then(|| quote!(<'_>));
    let set_offset_bytes = item
        .offset_host
        .is_some()
        .then(|| quote!(let offset_data = obj.bytes();));
    let field_inits = item
        .fields
        .iter()
        .filter(|fld| !fld.is_computed())
        .map(|fld| {
            let name = fld.name();
            let expr = fld.from_obj_expr();
            expr.map(|expr| quote!(#name: #expr))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let impl_to_owned_table = item.offset_host.is_some().then(|| {
        quote! {
            impl ToOwnedTable for super:: #name #lifetime {
                type Owned = #name;
            }
        }
    });
    let allow_dead = (item.offset_host.is_some() || !item.has_field_with_lifetime())
        .then(|| quote!(#[allow(unused_variables)]));

    Ok(quote! {
        impl FromObjRef<super::#name #lifetime> for #name {
            #allow_dead
            fn from_obj(obj: &super::#name #lifetime, offset_data: &[u8]) -> Option<Self> {
                #set_offset_bytes
                Some(#name {
                    #(#field_inits,)*
                })
            }
        }

        #impl_to_owned_table
    })
}

fn generate_group(group: &parse::ItemGroup) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &group.name;
    let variants = group.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = &variant.typ;
        quote!(#name(#typ))
    });

    let first_variant = &group
        .variants
        .iter()
        .next()
        .ok_or_else(|| syn::Error::new(name.span(), "empty enums are not allowed"))?
        .name;

    let impl_from_obj = group_from_obj(group)?;

    Ok(quote! {
        #[derive(Debug)]
        pub enum #name {
            #(#variants),*
        }

        #impl_from_obj

        impl Default for #name {
            fn default() -> Self {
                Self::#first_variant(Default::default())
            }
        }
    })
}

fn group_from_obj(group: &parse::ItemGroup) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &group.name;
    let lifetime = group.lifetime.is_some().then(|| quote!(<'_>));
    let match_arms = group.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!(super::#name::#var_name(item) => Self::#var_name(FromObjRef::from_obj(item, offset_data)?))
    });

    let impl_to_owned_table = group.offset_host.is_some().then(|| {
        quote! {
            impl ToOwnedTable for super:: #name #lifetime {
                type Owned = #name;
            }
        }
    });

    Ok(quote! {
        impl FromObjRef<super::#name #lifetime> for #name {
            fn from_obj(obj: &super::#name #lifetime, offset_data: &[u8]) -> Option<Self> {
                Some(match obj {
                    #(#match_arms,)*
                })
            }
        }

        #impl_to_owned_table
    })
}
