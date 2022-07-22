//! methods on fields

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use crate::lib2::parsing::Count;

use super::parsing::{Field, FieldType, Fields};

impl Fields {
    pub(crate) fn new(mut fields: Vec<Field>) -> syn::Result<Self> {
        let referenced_fields = fields
            .iter()
            .flat_map(Field::input_fields)
            .cloned()
            .collect::<HashSet<_>>();

        for field in fields.iter_mut() {
            field.read_at_parse_time =
                field.attrs.version.is_some() || referenced_fields.contains(&field.name);
        }

        Ok(Fields { fields })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }

    pub(crate) fn iter_compile_decls(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().filter_map(Field::compile_field_decl)
    }

    pub(crate) fn iter_compile_write_stmts(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().map(Field::compile_write_stmt)
    }
}

impl Field {
    pub(crate) fn type_for_record(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => quote!(BigEndian<#typ>),
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

    fn is_nullable(&self) -> bool {
        self.attrs.nullable.is_some()
    }

    fn is_computed(&self) -> bool {
        self.attrs.format.is_some() || self.attrs.compile.is_some()
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
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
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
            .flat_map(|count| count.iter_referenced_fields())
            .chain(
                self.attrs
                    .len
                    .as_ref()
                    .into_iter()
                    .flat_map(|expr| expr.referenced_fields.iter()),
            )
    }

    /// 'raw' as in this does not include handling offset resolution
    pub(crate) fn raw_getter_return_type(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => typ.to_token_stream(),
            FieldType::Other { typ } => quote!( &#typ ),
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
                    quote!(&[BigEndian<#typ>])
                }
                FieldType::Other { typ } => quote!( &[#typ] ),
                _ => unreachable!(),
            },
        }
    }

    pub(crate) fn owned_type(&self) -> TokenStream {
        self.typ.compile_type(self.is_nullable())
    }

    pub(crate) fn field_getter(&self) -> Option<TokenStream> {
        if !self.has_getter() {
            return None;
        }

        let name = &self.name;
        let is_array = self.is_array();
        let is_versioned = self.is_version_dependent();

        let mut return_type = self.raw_getter_return_type();
        if is_versioned {
            return_type = quote!(Option<#return_type>);
        }

        let range_stmt = self.getter_range_stmt();
        let mut read_stmt = if is_array {
            quote!(self.data.read_array(range).unwrap())
        } else {
            quote!(self.data.read_at(range.start).unwrap())
        };
        if is_versioned {
            read_stmt = quote!(Some(#read_stmt));
        }

        let docs = &self.attrs.docs;
        let offset_getter = self.typed_offset_field_getter();

        Some(quote! {
            #( #docs )*
            pub fn #name(&self) -> #return_type {
                let range = #range_stmt;
                #read_stmt
            }

            #offset_getter
        })
    }

    fn getter_range_stmt(&self) -> TokenStream {
        let shape_range_fn_name = self.shape_byte_range_fn_name();
        let try_op = self.is_version_dependent().then(|| quote!(?));
        quote!( self.shape.#shape_range_fn_name() #try_op )
    }

    fn typed_offset_field_getter(&self) -> Option<TokenStream> {
        let (typ, target) = match &self.typ {
            _ if self.attrs.no_offset_getter.is_some() => return None,
            FieldType::Offset {
                typ,
                target: Some(target),
            } => (typ, target),
            _ => return None,
        };

        let getter_name = self.offset_getter_name().unwrap();
        let mut return_type = quote!(Result<#target<'a>, ReadError>);
        if self.is_nullable() || self.attrs.available.is_some() {
            return_type = quote!(Option<#return_type>);
        }
        let range_stmt = self.getter_range_stmt();
        let resolve_method = self
            .is_nullable()
            .then(|| quote!(resolve_nullable))
            .unwrap_or_else(|| quote!(resolve));

        let return_stmt = if self.is_version_dependent() && !self.is_nullable() {
            quote!(Some(result))
        } else {
            quote!(result)
        };

        let raw_name = &self.name;
        let docs = format!(" Attempt to resolve [`{raw_name}`][Self::{raw_name}].");

        Some(quote! {
            #[doc = #docs]
            pub fn #getter_name(&self) -> #return_type {
                let range = #range_stmt;
                let offset: #typ = self.data.read_at(range.start).unwrap();
                let result = offset.#resolve_method(&self.data);
                #return_stmt
            }
        })
    }

    fn offset_getter_name(&self) -> Option<syn::Ident> {
        if !matches!(self.typ, FieldType::Offset { .. }) {
            return None;
        }
        let name_string = self.name.to_string();
        let offset_name = name_string
            .trim_end_matches("_offsets")
            .trim_end_matches("_offset");
        Some(syn::Ident::new(offset_name, self.name.span()))
    }

    /// the code generated for this field to validate data at parse time.
    pub(crate) fn field_parse_validation_stmts(&self) -> TokenStream {
        let name = &self.name;
        //dbg!(&name);
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
                let expr = match self.attrs.count.as_ref() {
                    Some(Count::Field(field)) => quote!( (#field as usize )),
                    Some(Count::Expr(expr)) => expr.expr.to_token_stream(),
                    None => unreachable!("must have one of count/count_exr/len"),
                };
                let inner_type = self.typ.inner_type().expect("only arrays have count attr");
                quote!(  #expr * #inner_type::RAW_BYTE_LEN )
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

    /// 'None' if this field's value is computed at compile time
    fn compile_field_decl(&self) -> Option<TokenStream> {
        if self.is_computed() {
            return None;
        }

        let name = &self.name;
        let docs = &self.attrs.docs;
        let typ = self.owned_type();
        Some(quote!( #( #docs)* pub #name: #typ ))
    }

    fn compile_write_stmt(&self) -> TokenStream {
        let value_expr = if let Some(format) = &self.attrs.format {
            let typ = self.typ.compile_type(self.is_nullable());
            let value = &format.value;
            quote!( (#value as #typ) )
        } else if let Some(computed) = &self.attrs.compile {
            let expr = computed.compile_expr();
            if !computed.referenced_fields.is_empty() {
                quote!( #expr.unwrap() )
            } else {
                quote!( #expr )
            }
            // not computed
        } else {
            let name = &self.name;
            quote!( self.#name )
        };

        quote!(#value_expr.write_into(writer))
    }
}

impl FieldType {
    /// 'cooked', as in now 'raw', i.e no 'BigEndian' wrapper
    pub(crate) fn cooked_type_tokens(&self) -> &syn::Ident {
        match &self {
            FieldType::Offset { typ, .. }
            | FieldType::Scalar { typ }
            | FieldType::Other { typ } => typ,

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

    fn compile_type(&self, nullable: bool) -> TokenStream {
        match self {
            FieldType::Scalar { typ } | FieldType::Other { typ } => typ.into_token_stream(),
            FieldType::Offset { typ, target } => {
                let target = target
                    .as_ref()
                    .map(|t| t.into_token_stream())
                    .unwrap_or_else(|| quote!(Box<dyn FontWrite>));
                if nullable {
                    quote!(NullableOffsetMarker<#typ, #target>)
                } else {
                    quote!(OffsetMarker<#typ, #target>)
                }
            }
            FieldType::Array { inner_typ } => {
                if matches!(inner_typ.as_ref(), &FieldType::Array { .. }) {
                    panic!("nesting arrays is not supported");
                }

                let inner_tokens = inner_typ.compile_type(nullable);
                quote!( Vec<#inner_tokens> )
            }
        }
    }
}
