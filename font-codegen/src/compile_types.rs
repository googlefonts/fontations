use quote::quote;

use crate::parse;

pub fn generate_compile_module(
    parsed: &parse::Items,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let items = parsed
        .items
        .iter()
        .flat_map(|item| match item {
            parse::Item::Single(item) if item.no_compile.is_none() => {
                Some(generate_single_item(item))
            }
            parse::Item::Group(item) => Some(generate_group(item)),
            parse::Item::Flags(item) => Some(generate_flags(item)),
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;

    let use_paths = parsed.use_stmts.iter().map(|stmt| stmt.compile_use_stmt());
    // we use these types directly, so reexport from compile directory
    let use_manual_impls = parsed.items.iter().filter_map(|item| match item {
        parse::Item::RawEnum(parse::RawEnum { name, .. })
        | parse::Item::Flags(parse::BitFlags { name, .. }) => Some(quote!(super::#name)),
        _ => None,
    });

    let custom_compile_types = &parsed.compile_types;
    Ok(quote! {
            #[allow(unused_imports)]
            use crate::compile::*;
            #[allow(unused_imports)]
            use font_types::*;
            #(use #use_paths;)*
            #(use #use_manual_impls;)*

            #(#custom_compile_types)*

            #(#items)*
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

    let impl_to_owned = item
        .skip_to_owned
        .is_none()
        .then(|| item_to_owned(item))
        .transpose()?;
    let impl_font_write = item_font_write(item)?;

    Ok(quote! {
        #[derive(Debug, PartialEq)]
        pub struct #name {
            #(#field_decls,)*
        }

        #impl_to_owned

        #impl_font_write
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

    Ok(quote! {
        impl ToOwnedObj for super::#name #lifetime {
            type Owned = #name;
            #[allow(unused_variables)]
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

    let impl_to_owned = group_to_owned(group)?;
    let impl_font_write = group_font_write(group)?;

    Ok(quote! {
        #[derive(Debug, PartialEq)]
        pub enum #name {
            #(#variants),*
        }

        #impl_to_owned

        #impl_font_write

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

fn generate_flags(item: &parse::BitFlags) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &item.name;
    Ok(quote! {
        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                self.bits().write_into(writer)
            }
        }
    })
}
