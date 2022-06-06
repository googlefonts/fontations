use quote::quote;

use crate::parse;

pub fn generate_compile_module(
    parsed: &parse::Items,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let items = parsed
        .items
        .iter()
        .flat_map(|item| match item {
            parse::Item::Single(item) if item.manual_compile_type.is_none() => {
                Some(generate_single_item(item))
            }
            parse::Item::Group(item) => Some(generate_group(item)),
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;

    let use_paths = parsed.use_stmts.iter().map(|stmt| stmt.compile_use_stmt());
    let use_manual_impls = parsed.items.iter().filter_map(|item| match item {
        parse::Item::Single(item) if item.manual_compile_type.is_some() => {
            let name = &item.name;
            Some(quote!(super::super::compile::#name))
        }
        _ => None,
    });
    Ok(quote! {
        #[cfg(feature = "compile")]
        pub mod compile {
            use crate::compile::*;
            use font_types::{Offset as _, OffsetHost as _};
            #(use #use_paths;)*
            #(use #use_manual_impls;)*

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

    let generate_compile_traits = item.manual_compile_type.is_none();
    let impl_to_owned = generate_compile_traits
        .then(|| item_to_owned(item))
        .transpose()?;

    let impl_font_write = generate_compile_traits
        .then(|| item_font_write(item))
        .transpose()?;

    Ok(quote! {
        #[derive(Debug, Default, PartialEq)]
        pub struct #name {
            #(#field_decls,)*
        }

        #impl_to_owned

        #impl_font_write

        impl #name {
            pub fn new() -> Self {
                Default::default()
            }
        }
    })
}

fn item_to_owned(item: &parse::SingleItem) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &item.name;
    let lifetime = item.lifetime.is_some().then(|| quote!(<'_>));
    let set_offset_bytes = item
        .offset_host
        .is_some()
        .then(|| quote!(let offset_data = self.bytes();));
    let field_inits = item
        .fields
        .iter()
        .filter(|fld| !fld.is_computed())
        .map(|fld| {
            let name = fld.name();
            let expr = fld.to_owned_expr();
            expr.map(|expr| quote!(#name: #expr))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let impl_to_owned_table = item
        .offset_host
        .is_some()
        .then(|| quote!(impl ToOwnedTable for super:: #name #lifetime {}));
    let allow_dead = (item.offset_host.is_some() || !item.has_field_with_lifetime())
        .then(|| quote!(#[allow(unused_variables)]));

    Ok(quote! {
        impl ToOwnedObj for super::#name #lifetime {
            type Owned = #name;
            #allow_dead
            fn to_owned_obj(&self, offset_data: &[u8]) -> Option<Self::Owned> {
                #set_offset_bytes
                Some(#name {
                    #(#field_inits,)*
                })
            }
        }

        #impl_to_owned_table
    })
}

fn item_font_write(item: &parse::SingleItem) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &item.name;

    let field_exprs = item.fields.iter().map(|fld| fld.font_write_expr());

    Ok(quote! {
        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                #(#field_exprs)*

            }
        }

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

    let impl_to_owned = group_to_owned(group)?;
    let impl_font_write = group_font_write(group)?;

    Ok(quote! {
        #[derive(Debug, PartialEq)]
        pub enum #name {
            #(#variants),*
        }

        #impl_to_owned

        #impl_font_write

        impl Default for #name {
            fn default() -> Self {
                Self::#first_variant(Default::default())
            }
        }
    })
}

fn group_to_owned(group: &parse::ItemGroup) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &group.name;
    let lifetime = group.lifetime.is_some().then(|| quote!(<'_>));
    let match_arms = group.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!(super::#name::#var_name(item) => #name::#var_name(item.to_owned_obj(offset_data)?))
    });

    let impl_to_owned_table = group
        .offset_host
        .is_some()
        .then(|| quote!(impl ToOwnedTable for super:: #name #lifetime {}));

    Ok(quote! {
        impl ToOwnedObj for super::#name #lifetime {
            type Owned = #name;
            fn to_owned_obj(&self, offset_data: &[u8]) -> Option<Self::Owned> {
                Some(match self {
                    #(#match_arms,)*
                })
            }
        }

        #impl_to_owned_table
    })
}

fn group_font_write(group: &parse::ItemGroup) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &group.name;
    let match_arms = group.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.write_into(writer), )
    });

    Ok(quote! {
        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                match self {
                    #(#match_arms)*
                }
            }
        }
    })
}
