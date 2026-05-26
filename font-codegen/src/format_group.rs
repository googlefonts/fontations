//! codegen for format group types

use std::collections::HashMap;

use indexmap::IndexMap;
use proc_macro2::TokenStream;
use quote::quote;

use crate::fields::FieldConstructorInfo;
use crate::parsing::{logged_syn_error, Field, Item, Items, TableFormat};

pub(crate) fn generate(item: &TableFormat, items: &Items) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let variants = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let name = &variant.name;
            let typ = variant.type_name();
            let docs = &variant.attrs.docs;
            quote! ( #( #docs )* #name(#typ<'a>) )
        });

    let format = &item.format;
    // if we have any fancy match statement we disable a clippy lint
    let mut has_any_match_stmt = false;
    let match_arms = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let name = &variant.name;
            let lhs = if let Some(expr) = variant.attrs.match_stmt.as_deref() {
                has_any_match_stmt = true;
                let expr = &expr.expr;
                quote!(format if #expr)
            } else {
                let typ = variant.type_name();
                quote!(#typ::FORMAT)
            };
            Some(quote! {
                #lhs => {
                    Ok(Self::#name(FontRead::read(data)?))
                }
            })
        })
        .collect::<Vec<_>>();

    let maybe_allow_lint = has_any_match_stmt.then(|| quote!(#[allow(clippy::redundant_guards)]));

    let traversal_arms = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let name = &variant.name;
            quote!(Self::#name(table) => table)
        });

    let format_offset = item.format_offset();

    let getters = generate_shared_getters(item, items)?;
    let getters = (!getters.is_empty()).then(|| {
        quote! {
            impl<'a> #name<'a> {
                #getters
            }
        }
    });

    let mut min_byte_arms = Vec::new();
    let mut min_table_byte_arms = Vec::new();
    for variant in item
        .variants
        .iter()
        .filter(|v| v.attrs.write_only.is_none())
    {
        let var_name: &syn::Ident = &variant.name;
        min_byte_arms.push(quote!(Self::#var_name(item) => item.min_byte_range(), ));
        min_table_byte_arms.push(quote!(Self::#var_name(item) => item.min_table_bytes(), ));
    }
    let first_variant = item.variants.first().expect("format group needs variants");
    // this attribute is currently used only once and we expect it to only ever
    // be the last variant, but let's be defensive
    assert!(first_variant.attrs.write_only.is_none(), "sanity check");
    let first_var_name = &first_variant.name;

    let sanitize = items.sanitize.then(|| generate_sanitize(item));

    Ok(quote! {
        #( #docs )*
        #[derive(Clone)]
        pub enum #name<'a> {
            #( #variants ),*
        }

        impl Default for #name<'_> {
            fn default() -> Self {
                Self::#first_var_name(Default::default())
            }
        }


        #getters

        impl<'a> FontRead<'a> for #name<'a> {
            fn read(data: FontData<'a>) -> Result<Self, ReadError> {
                let format: #format = data.read_at(#format_offset)?;
                #maybe_allow_lint
                match format {
                    #( #match_arms ),*
                    other => Err(ReadError::InvalidFormat(other.into())),
                }
            }
        }

        impl<'a> MinByteRange<'a> for #name<'a> {
            fn min_byte_range(&self) -> Range<usize> {
                match self {
                    #( #min_byte_arms )*
                }
            }
            fn min_table_bytes(&self) -> &'a [u8] {
                match self {
                    #( #min_table_byte_arms )*
                }
            }
        }

        #sanitize

        #[cfg(feature = "experimental_traverse")]
        impl<'a> #name<'a> {
            fn dyn_inner<'b>(&'b self) -> &'b dyn SomeTable<'a> {
                match self {
                    #( #traversal_arms, )*
                }
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl std::fmt::Debug for #name<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.dyn_inner().fmt(f)
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> SomeTable<'a> for #name<'a> {
            fn type_name(&self) -> &str {
                self.dyn_inner().type_name()
            }

            fn get_field(&self, idx: usize) -> Option<Field<'a>> {
                self.dyn_inner().get_field(idx)
            }
        }
    })
}

fn generate_sanitize(item: &TableFormat) -> TokenStream {
    let name = &item.name;
    let format = &item.format;
    let format_offset = item.format_offset();

    let mut has_any_match_stmt = false;
    let match_arms: Vec<_> = item
        .variants
        .iter()
        .filter(|v| v.attrs.write_only.is_none())
        .map(|variant| {
            let typ = variant.type_name();
            let lhs = if let Some(expr) = variant.attrs.match_stmt.as_deref() {
                has_any_match_stmt = true;
                let expr = &expr.expr;
                quote!(format if #expr)
            } else {
                quote!(#typ::FORMAT)
            };
            quote!(#lhs => #typ::sanitize(ctx, ()),)
        })
        .collect();

    let maybe_allow_lint = has_any_match_stmt.then(|| quote!(#[allow(clippy::redundant_guards)]));

    quote! {
        impl Sanitize for #name<'_> {
            fn sanitize(ctx: &mut SanitizeContext, _args: ()) -> Result<(), ReadError> {
                let format: #format = ctx.peek_at(#format_offset)?;
                #maybe_allow_lint
                match format {
                    #( #match_arms )*
                    other => Err(ReadError::InvalidFormat(other.into())),
                }
            }
        }
    }
}

pub(crate) fn generate_compile(item: &TableFormat, items: &Items) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let parse_module = &items.parse_module_path;
    let variants = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = variant.type_name();
        let docs = &variant.attrs.docs;
        quote! ( #( #docs )* #name(#typ) )
    });

    let default_variant = &item.variants.first().unwrap().name;

    let write_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.write_into(writer), )
    });

    let validation_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.validate_impl(ctx), )
    });

    let table_type_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.table_type(), )
    });

    let from_impls = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        let typ = variant.type_name();
        quote!( impl From<#typ> for #name {
            fn from(src: #typ) -> #name {
                #name::#var_name(src)
            }
        } )
    });

    let from_obj_impl = item
        .attrs
        .skip_from_obj
        .is_none()
        .then(|| generate_from_obj(item, parse_module))
        .transpose()?;

    let constructors = generate_constructors(item, items)?;
    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum #name {
            #( #variants ),*
        }

        #constructors

        impl Default for #name {
            fn default() -> Self {
                Self::#default_variant(Default::default())
            }
        }

        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                match self {
                    #( #write_arms )*
                }
            }

            fn table_type(&self) -> TableType {
                match self {
                    #( #table_type_arms )*
                }
            }
        }

        impl Validate for #name {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                match self {
                    #( #validation_arms )*
                }
            }
        }

        #from_obj_impl

        #( #from_impls )*

    })
}

fn generate_constructors(item: &TableFormat, items: &Items) -> syn::Result<TokenStream> {
    let mut constructors = Vec::new();
    let name = &item.name;

    for variant in &item.variants {
        let var_name = &variant.name;
        let var_type = variant.type_name();

        let Some(Item::Table(table)) = items.get(var_type) else {
            return Err(logged_syn_error(var_type.span(), "Unknown type; codegen currently expects types in format groups to be local to the file."));
        };
        if table.attrs.skip_constructor.is_some() {
            continue;
        }

        let constructor_args_raw = table.fields.iter_constructor_info().collect::<Vec<_>>();
        let constructor_args = constructor_args_raw.iter().map(
            |FieldConstructorInfo {
                 name, arg_tokens, ..
             }| quote!(#name: #arg_tokens),
        );
        let constructor_arg_names = constructor_args_raw.iter().map(|info| &info.name);

        let constructor_ident = make_snake_case_ident(var_name);

        let docstring = format!(" Construct a new `{}` subtable", variant.type_name());
        // judiciously allow this lint
        let too_many_args =
            (constructor_args.len() > 7).then(|| quote!(#[allow(clippy::too_many_arguments)]));
        constructors.push(quote! {
             #[doc = #docstring]
            #too_many_args
            pub fn #constructor_ident ( #( #constructor_args,)*  ) -> Self {
                Self::#var_name( #var_type::new( #( #constructor_arg_names, )* ))
            }
        });
    }

    Ok(quote! {
        impl #name {

            #( #constructors )*
        }
    })
}

fn generate_shared_getters(item: &TableFormat, items: &Items) -> syn::Result<TokenStream> {
    // okay so we want to identify the getters that exist on all variants.
    let all_variants = item
        .variants
        .iter()
        .map(|var| {
            let type_name = var.type_name();
            match items.get(type_name) {
                Some(Item::Table(item)) => Ok(item),
                _ => Err(logged_syn_error(
                    type_name.span(),
                    "must be a table defined in this file",
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    // okay so now we have all of the actual inner types, and we need to find which
    // getters are shared between all of them
    let mut field_counts = IndexMap::new();
    let mut all_fields = HashMap::new();
    for table in &all_variants {
        for field in table.fields.iter().filter(|fld| fld.has_getter()) {
            let key = (&field.name, &field.typ);
            // we have to convert the tokens to a string to get hash/ord/etc
            *field_counts.entry(key).or_insert(0usize) += 1;
            all_fields.entry(&field.name).or_insert(field);
        }
    }

    let shared_fields = field_counts
        .into_iter()
        .filter(|(_, count)| *count == all_variants.len())
        .map(|((name, _), _)| all_fields.get(name).unwrap())
        .collect::<Vec<_>>();

    let getters = shared_fields
        .iter()
        .map(|fld| generate_getter_for_shared_field(item, fld));

    // and we also want to have a wrapper for offset_data():
    let data_arms = item
        .variants
        .iter()
        .filter(|v| v.attrs.write_only.is_none())
        .map(|variant| {
            let var_name = &variant.name;
            quote!(Self::#var_name(item) => item.offset_data(), )
        });

    // now we have a collection of fields present on all variants, and
    // we need to actually generate the wrapping getter

    Ok(quote! {
        #[doc = "Return the `FontData` used to resolve offsets for this table."]
        pub fn offset_data(&self) -> FontData<'a> {
            match self {
                #( #data_arms )*
            }
        }
        #( #getters )*
    })
}

fn generate_getter_for_shared_field(item: &TableFormat, field: &Field) -> TokenStream {
    let docs = &field.attrs.docs;
    let method_name = &field.name;
    let return_type = field.table_getter_return_type();
    let arms = item.variants.iter().map(|variant| {
        let var_name: &syn::Ident = &variant.name;
        quote!(Self::#var_name(item) => item.#method_name(), )
    });

    // but we also need to handle offset getters, and that's a pain

    quote! {
        #( #docs )*
        pub fn #method_name(&self) -> #return_type {
            match self {
                #( #arms )*
            }
        }
    }
}

fn generate_from_obj(item: &TableFormat, parse_module: &syn::Path) -> syn::Result<TokenStream> {
    let name = &item.name;
    let to_owned_arms = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let var_name = &variant.name;
            quote!( ObjRefType::#var_name(item) => #name::#var_name(item.to_owned_table()), )
        });

    Ok(quote! {
        impl FromObjRef<#parse_module:: #name<'_>> for #name {
            fn from_obj_ref(obj: &#parse_module:: #name, _: FontData) -> Self {
                use #parse_module::#name as ObjRefType;
                match obj {
                    #( #to_owned_arms )*
                }
            }
        }

        impl FromTableRef<#parse_module::#name<'_>> for #name {}

        impl<'a> FontRead<'a> for #name {
            fn read(data: FontData<'a>) -> Result<Self, ReadError> {
                <#parse_module :: #name as FontRead>::read(data)
                    .map(|x| x.to_owned_table())
            }
        }
    })
}

impl TableFormat {
    fn format_offset(&self) -> usize {
        self.format_offset
            .as_ref()
            .map(|lit| {
                lit.base10_parse::<usize>()
                    .expect("format offset must be unsigned")
            })
            .unwrap_or(0)
    }
}
// An overwrought and likely incorrect way of converting 'Format1' to 'format_1' -_-
fn make_snake_case_ident(ident: &syn::Ident) -> syn::Ident {
    let input = ident.to_string();
    let mut output = String::with_capacity(input.len() + 2);
    let mut prev_char = input.chars().next().unwrap();
    output.extend(prev_char.to_lowercase());
    for c in input.chars().skip(1) {
        if (c.is_uppercase() && !prev_char.is_uppercase())
            || (c.is_numeric() && !prev_char.is_numeric())
        {
            output.push('_');
        }
        output.extend(c.to_lowercase());
        prev_char = c;
    }

    syn::Ident::new(&output, ident.span())
}
