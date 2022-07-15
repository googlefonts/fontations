//! methods on fields

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use super::parsing::{Field, FieldType, Fields};

impl Fields {
    pub(crate) fn new(mut fields: Vec<Field>) -> syn::Result<Self> {
        let referenced_fields = fields
            .iter()
            .flat_map(Field::input_fields)
            .cloned()
            .collect::<HashSet<_>>();

        for field in fields.iter_mut() {
            field.read_at_parse_time = field.attrs.format.is_some()
                || field.attrs.version.is_some()
                || referenced_fields.contains(&field.name);
        }

        Ok(Fields { fields })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }
}

impl Field {
    pub(crate) fn type_for_record(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ } | FieldType::Scalar { typ } => quote!(BigEndian<#typ>),
            _ => panic!("arrays and custom types not supported in records"),
        }
    }

    pub(crate) fn shape_byte_range_fn_name(&self) -> syn::Ident {
        quote::format_ident!("{}_byte_range", &self.name)
    }

    pub(crate) fn shape_byte_len_field_name(&self) -> syn::Ident {
        quote::format_ident!("{}_byte_len", &self.name)
    }

    pub(crate) fn shape_byte_start_field_name(&self) -> syn::Ident {
        // used when fields are optional
        quote::format_ident!("{}_byte_start", &self.name)
    }

    pub(crate) fn is_array(&self) -> bool {
        matches!(&self.typ, FieldType::Array { .. })
    }

    pub(crate) fn has_computed_len(&self) -> bool {
        self.attrs.len.is_some() || self.attrs.count.is_some()
    }

    pub(crate) fn is_version_dependent(&self) -> bool {
        self.attrs.available.is_some()
    }

    pub(crate) fn validate_at_parse(&self) -> bool {
        false
        //FIXME: validate fields?
        //self.attrs.format.is_some()
    }

    pub(crate) fn has_getter(&self) -> bool {
        self.attrs.no_getter.is_none()
    }

    pub(crate) fn len_expr(&self) -> TokenStream {
        // is this a scalar/offset? then it's just 'RAW_BYTE_LEN'
        // is this computed? then it is stored
        match &self.typ {
            FieldType::Offset { typ } | FieldType::Scalar { typ } => {
                quote!(#typ::RAW_BYTE_LEN)
            }
            FieldType::Other { .. } | FieldType::Array { .. } => {
                let len_field = self.shape_byte_len_field_name();
                quote!(self.#len_field)
            }
        }
    }

    /// iterate other named fields that are used as in input to a calculation
    /// done when parsing this field.
    fn input_fields(&self) -> impl Iterator<Item = &syn::Ident> {
        self.attrs
            .count
            .as_ref()
            .into_iter()
            .flat_map(|expr| expr.referenced_fields.iter())
            .chain(
                self.attrs
                    .len
                    .as_ref()
                    .into_iter()
                    .flat_map(|expr| expr.referenced_fields.iter()),
            )
    }

    pub(crate) fn getter_return_type(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ } | FieldType::Scalar { typ } => typ.to_token_stream(),
            FieldType::Other { typ } => quote!( &#typ ),
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ } | FieldType::Scalar { typ } => quote!(&[BigEndian<#typ>]),
                FieldType::Other { typ } => quote!( &[#typ] ),
                _ => unreachable!(),
            },
        }
    }

    /// the code generated for this field to validate data at parse time.
    pub(crate) fn field_parse_validation_stmts(&self) -> TokenStream {
        let name = &self.name;
        // handle the trivial case
        if !self.read_at_parse_time
            && !self.has_computed_len()
            && !self.validate_at_parse()
            && !self.is_version_dependent()
        {
            let typ = self.typ.cooked_type_tokens();
            return quote!( cursor.advance::<#typ>(); );
        }

        let versioned_field_start = self.attrs.available.as_ref().map(|available|{
            let field_start_name = self.shape_byte_start_field_name();
            quote! ( let #field_start_name = version.compatible(#available).then(|| cursor.position()).transpose()?; )
        });

        let other_stuff = if self.has_computed_len() {
            assert!(!self.read_at_parse_time, "i did not expect this to happen");
            let len_field_name = self.shape_byte_len_field_name();
            let len_expr = if let Some(expr) = &self.attrs.len {
                expr.expr.to_token_stream()
            } else {
                let count_expr = &self
                    .attrs
                    .count
                    .as_ref()
                    .expect("must have one of count or len")
                    .expr;
                let inner_type = self.typ.inner_type().expect("only arrays have count attr");
                quote! ( (#count_expr) as usize * #inner_type::RAW_BYTE_LEN )
            };

            match &self.attrs.available {
                Some(version) => quote! {
                    let #len_field_name = version.compatible(#version).then(|| #len_expr);
                    #len_field_name.map(|value| cursor.advance_by(value));
                },
                None => quote! {
                    let #len_field_name = #len_expr;
                    cursor.advance_by(#len_field_name);
                },
            }
        } else if self.read_at_parse_time {
            assert!(!self.is_version_dependent(), "does this happen?");
            let typ = self.typ.cooked_type_tokens();
            quote! ( let #name: #typ = cursor.read()?; )
        } else if let Some(available) = &self.attrs.available {
            assert!(!self.is_array());
            let typ = self.typ.cooked_type_tokens();
            quote! {
            version.compatible(#available).then(|| cursor.advance::<#typ>());
            }
        } else {
            panic!("who wrote this garbage anyway?");
        };

        quote! {
            #versioned_field_start
            #other_stuff
        }
    }
}

impl FieldType {
    /// 'cooked', as in now 'raw', i.e no 'BigEndian' wrapper
    pub(crate) fn cooked_type_tokens(&self) -> &syn::Ident {
        match &self {
            FieldType::Offset { typ } | FieldType::Scalar { typ } | FieldType::Other { typ } => typ,

            FieldType::Array { .. } => panic!("array tokens never cooked"),
        }
    }

    pub(crate) fn inner_type(&self) -> Option<&syn::Ident> {
        if let FieldType::Array { inner_typ } = &self {
            Some(inner_typ.cooked_type_tokens())
        } else {
            None
        }
    }
}
