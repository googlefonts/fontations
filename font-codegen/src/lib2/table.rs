//! codegen for table objects

use proc_macro2::TokenStream;
use quote::quote;

use super::parsing::{Field, Table, TableFormat};

pub(crate) fn generate(item: &Table) -> syn::Result<TokenStream> {
    let marker_name = &item.name;
    let shape_name = item.shape_name();
    let shape_byte_range_fns = item.iter_shape_byte_fns();
    let shape_fields = item.iter_shape_fields();

    let field_validation_stmts = item.iter_field_validation_stmts();
    let shape_field_names = item.iter_shape_field_names();

    let table_ref_getters = item.iter_table_ref_getters();

    let optional_format_trait_impl = item.impl_format_trait();

    Ok(quote! {
        #[derive(Debug, Clone, Copy)]
        pub struct #marker_name;

        #optional_format_trait_impl

        #[derive(Debug, Clone, Copy)]
        pub struct #shape_name {
            #( #shape_fields )*
        }

        impl #shape_name {
            #( #shape_byte_range_fns )*
        }

        impl TableInfo for #marker_name {
            type Info = #shape_name;

            fn parse<'a>(data: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
                let mut cursor = data.cursor();
                #( #field_validation_stmts )*
                cursor.finish( #shape_name {
                    #( #shape_field_names, )*
                })
            }
        }

        impl<'a> TableRef<'a, #marker_name> {

            #( #table_ref_getters )*

        }
    })
}

pub(crate) fn generate_format_group(item: &TableFormat) -> syn::Result<TokenStream> {
    let name = &item.name;
    let variants = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = &variant.typ;
        quote! ( #name(TableRef<'a, #typ>) )
    });

    let format = &item.format;
    let match_arms = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = &variant.typ;
        quote! {
            <#typ as Format<#format>>::FORMAT => {
                Ok(Self::#name(FontRead::read(data)?))
            }
        }
    });

    Ok(quote! {
        pub enum #name<'a> {
            #( #variants ),*
        }

        impl<'a> FontRead<'a> for #name<'a> {
            fn read(data: &FontData<'a>) -> Result<Self, ReadError> {
                let format: #format = data.read_at(0)?;
                match format {
                    #( #match_arms ),*
                    other => Err(ReadError::InvalidFormat(other)),
                }
            }
        }
    })
}

impl Table {
    fn shape_name(&self) -> syn::Ident {
        quote::format_ident!("{}Shape", &self.name)
    }

    fn iter_shape_byte_fns(&self) -> impl Iterator<Item = TokenStream> + '_ {
        let mut prev_field_end_expr = quote!(0);
        let mut iter = self.fields.iter();

        std::iter::from_fn(move || {
            let field = iter.next()?;
            let fn_name = field.shape_byte_range_fn_name();
            let len_expr = field.len_expr();

            // versioned fields have a different signature
            if field.attrs.available.is_some() {
                prev_field_end_expr = quote!(compile_error!(
                    "non-version dependent field cannot follow version-dependent field"
                ));
                let start_field_name = field.shape_byte_start_field_name();
                return Some(quote! {
                    fn #fn_name(&self) -> Option<Range<usize>> {
                        let start = self.#start_field_name?;
                        Some(start..start + #len_expr)
                    }
                });
            }

            let result = quote! {
                fn #fn_name(&self) -> Range<usize> {
                    let start = #prev_field_end_expr;
                    start..start + #len_expr
                }
            };
            prev_field_end_expr = quote!( self.#fn_name().end );

            Some(result)
        })
    }

    fn iter_shape_fields(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.iter_shape_field_names_and_types()
            .map(|(ident, typ)| quote!( #ident: #typ ))
    }

    fn iter_shape_field_names(&self) -> impl Iterator<Item = syn::Ident> + '_ {
        self.iter_shape_field_names_and_types()
            .map(|(name, _)| name)
    }

    fn iter_shape_field_names_and_types(
        &self,
    ) -> impl Iterator<Item = (syn::Ident, TokenStream)> + '_ {
        let mut iter = self.fields.iter();
        let mut return_me = None;

        // a given field can have 0, 1, or 2 shape fields.
        std::iter::from_fn(move || loop {
            if let Some(thing) = return_me.take() {
                return Some(thing);
            }

            let next = iter.next()?;
            let is_versioned = next.attrs.available.is_some();
            let has_computed_len = next.has_computed_len();
            if !(is_versioned || has_computed_len) {
                continue;
            }

            let start_field = is_versioned.then(|| {
                let field_name = next.shape_byte_start_field_name();
                (field_name, quote!(Option<usize>))
            });

            let len_field = has_computed_len.then(|| {
                let field_name = next.shape_byte_len_field_name();
                if is_versioned {
                    (field_name, quote!(Option<usize>))
                } else {
                    (field_name, quote!(usize))
                }
            });
            if start_field.is_some() {
                return_me = len_field;
                return start_field;
            } else {
                return len_field;
            }
        })
    }

    fn iter_field_validation_stmts(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().map(Field::field_parse_validation_stmts)
    }

    fn iter_table_ref_getters(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields
            .iter()
            .filter(|fld| fld.has_getter())
            .map(|fld| {
                let name = &fld.name;
                let return_type = fld.getter_return_type();
                let shape_range_fn_name = fld.shape_byte_range_fn_name();
                let is_array = fld.is_array();
                let is_versioned = fld.is_version_dependent();
                let read_stmt = if is_array {
                    quote!(self.data.read_array(range).unwrap())
                } else {
                    quote!(self.data.read_at(range.start).unwrap())
                };

                if is_versioned {
                    quote! {
                        pub fn #name(&self) -> Option<#return_type> {
                            let range = self.shape.#shape_range_fn_name()?;
                            Some(#read_stmt)
                        }
                    }
                } else {
                    quote! {
                        pub fn #name(&self) -> #return_type {
                            let range = self.shape.#shape_range_fn_name();
                            // we would like to skip this unwrap
                            #read_stmt
                        }
                    }
                }
            })
    }

    pub(crate) fn impl_format_trait(&self) -> Option<TokenStream> {
        let field = self.fields.iter().find(|fld| fld.attrs.format.is_some())?;
        let name = &self.name;
        let value = &field.attrs.format.as_ref().unwrap().value;
        let typ = field.typ.cooked_type_tokens();

        Some(quote! {
            impl Format<#typ> for #name {
                const FORMAT: #typ = #value;
            }
        })
    }
}
