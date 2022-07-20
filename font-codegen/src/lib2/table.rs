//! codegen for table objects

use proc_macro2::TokenStream;
use quote::quote;

use super::parsing::{Field, Table, TableFormat};

pub(crate) fn generate(item: &Table) -> syn::Result<TokenStream> {
    if item.attrs.skip_parse.is_some() {
        return Ok(Default::default());
    }
    let docs = &item.attrs.docs;
    let marker_name = item.marker_name();
    let raw_name = item.raw_name();
    let shape_byte_range_fns = item.iter_shape_byte_fns();
    let shape_fields = item.iter_shape_fields();

    let field_validation_stmts = item.iter_field_validation_stmts();
    let shape_field_names = item.iter_shape_field_names();

    let table_ref_getters = item.iter_table_ref_getters();

    let optional_format_trait_impl = item.impl_format_trait();
    // add this attribute if we're going to be generating expressions which
    // may trigger a warning
    let ignore_parens = item
        .fields
        .iter()
        .any(|fld| fld.has_computed_len())
        .then(|| quote!(#[allow(unused_parens)]));

    Ok(quote! {
        #optional_format_trait_impl

        #( #docs )*
        #[derive(Debug, Clone, Copy)]
        #[doc(hidden)]
        pub struct #marker_name {
            #( #shape_fields ),*
        }

        impl #marker_name {
            #( #shape_byte_range_fns )*
        }

        impl TableInfo for #marker_name {
            #ignore_parens
            fn parse<'a>(data: FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
                let mut cursor = data.cursor();
                #( #field_validation_stmts )*
                cursor.finish( #marker_name {
                    #( #shape_field_names, )*
                })
            }
        }

        #( #docs )*
        pub type #raw_name<'a> = TableRef<'a, #marker_name>;

        impl<'a> #raw_name<'a> {

            #( #table_ref_getters )*

        }
    })
}

pub(crate) fn generate_format_group(item: &TableFormat) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.docs;
    let variants = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = variant.type_name();
        let docs = &variant.docs;
        quote! ( #( #docs )* #name(#typ<'a>) )
    });

    let format = &item.format;
    let match_arms = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = variant.marker_name();
        quote! {
            #typ::FORMAT => {
                Ok(Self::#name(FontRead::read(data)?))
            }
        }
    });

    Ok(quote! {
        #( #docs )*
        pub enum #name<'a> {
            #( #variants ),*
        }

        impl<'a> FontRead<'a> for #name<'a> {
            fn read(data: FontData<'a>) -> Result<Self, ReadError> {
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
    fn marker_name(&self) -> syn::Ident {
        quote::format_ident!("{}Marker", self.raw_name())
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
        self.fields.iter().filter_map(|fld| fld.field_getter())
    }

    pub(crate) fn impl_format_trait(&self) -> Option<TokenStream> {
        let field = self.fields.iter().find(|fld| fld.attrs.format.is_some())?;
        let name = self.marker_name();
        let value = &field.attrs.format.as_ref().unwrap().value;
        let typ = field.typ.cooked_type_tokens();

        Some(quote! {
            impl Format<#typ> for #name {
                const FORMAT: #typ = #value;
            }
        })
    }
}
