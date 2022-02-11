#![allow(dead_code)]

use std::str::FromStr;

use quote::quote;
use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token, Attribute, Token,
};

pub struct Items(Vec<Item>);

pub struct Item {
    pub lifetime: bool,
    pub attrs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    I8,
    U8,
    I16,
    U16,
    U32,
    I32,
    U24,
    Fixed,
    F2Dot14,
    LongDateTime,
    Offset16,
    Offset24,
    Offset32,
    Tag,
    Version16Dot16,
}

pub enum Field {
    Scalar(ScalarField),
    Array(ArrayField),
}

pub struct ScalarField {
    pub name: syn::Ident,
    pub typ: ScalarType,
    pub hidden: Option<syn::Path>,
}

pub struct ArrayField {
    pub name: syn::Ident,
    pub inner_typ: syn::Ident,
    pub inner_lifetime: bool,
    pub count: Count,
    pub variable_size: Option<syn::Path>,
}

/// Annotations for how to calculate the count of an array.
pub enum Count {
    Field(syn::Ident),
    Function {
        fn_: syn::Path,
        args: Vec<syn::Ident>,
    },
}

impl Parse for Items {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let mut result = Vec::new();
        while !input.is_empty() {
            result.push(input.parse()?)
        }
        Ok(Self(result))
    }
}

impl Items {
    pub fn iter(&self) -> impl Iterator<Item = &Item> {
        self.0.iter()
    }
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let name: syn::Ident = input.parse()?;
        let lifetime = get_generics(input)?;
        let content;
        let _ = braced!(content in input);
        let fields = Punctuated::<Field, Token![,]>::parse_terminated(&content)?;
        let fields = fields.into_iter().collect();
        let item = Self {
            lifetime,
            attrs,
            name,
            fields,
        };
        item.validate()?;
        Ok(item)
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = AllAttrs::parse(&attrs)?;
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![:]>()?;

        if input.lookahead1().peek(token::Bracket) {
            let content;
            bracketed!(content in input);
            let typ = content.parse::<syn::Ident>()?;
            let lifetime = get_generics(&content)?;
            attrs.into_array(name, typ, lifetime).map(Field::Array)
        } else {
            attrs.into_scalar(name, input.parse()?).map(Field::Scalar)
        }
    }
}

#[derive(Default)]
struct AllAttrs {
    hidden: Option<syn::Path>,
    count: Option<Count>,
    variable_size: Option<syn::Path>,
}

impl AllAttrs {
    fn parse(attrs: &[syn::Attribute]) -> Result<AllAttrs, syn::Error> {
        let mut result = AllAttrs::default();
        for attr in attrs {
            //dbg!(attr);
            match attr.parse_meta()? {
                syn::Meta::Path(path) if path.is_ident("hidden") => {
                    result.hidden = Some(path.clone())
                }
                syn::Meta::Path(path) if path.is_ident("variable_size") => {
                    result.variable_size = Some(path.clone())
                }
                syn::Meta::List(list) if list.path.is_ident("count") => {
                    if let Some(syn::NestedMeta::Meta(syn::Meta::Path(p))) = list.nested.first() {
                        if let Some(ident) = p.get_ident() {
                            result.count = Some(Count::Field(ident.clone()));
                            continue;
                        }
                    }
                    return Err(syn::Error::new(
                        list.path.span(),
                        "count attribute should have format count(some_path)",
                    ));
                }
                syn::Meta::List(list) if list.path.is_ident("count_with") => {
                    let mut items = list.nested.iter();
                    if let Some(syn::NestedMeta::Meta(syn::Meta::Path(path))) = items.next() {
                        let args = items.map(expect_ident).collect::<Result<_, _>>()?;
                        assert!(result.count.is_none(), "I ONLY COUNT ONCE");
                        result.count = Some(Count::Function {
                            fn_: path.to_owned(),
                            args,
                        });
                        continue;
                    }
                    return Err(syn::Error::new(
                        list.path.span(),
                        "count_with attribute should have format count_with(path::to::fn, arg1, arg2)",
                    ));
                }
                other => return Err(syn::Error::new(other.span(), "unknown attribute")),
            }
        }
        Ok(result)
    }

    fn into_array(
        self,
        name: syn::Ident,
        inner_typ: syn::Ident,
        inner_lifetime: bool,
    ) -> Result<ArrayField, syn::Error> {
        if let Some(path) = &self.hidden {
            return Err(syn::Error::new(
                path.span(),
                "'hidden' is only valid on scalar fields",
            ));
        }
        let count = self.count.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "array types require 'count' or 'count_with' attribute",
            )
        })?;
        let variable_size = self.variable_size;
        Ok(ArrayField {
            name,
            inner_typ,
            inner_lifetime,
            count,
            variable_size,
        })
    }

    fn into_scalar(self, name: syn::Ident, typ: ScalarType) -> Result<ScalarField, syn::Error> {
        if let Some(span) = self.count.as_ref().map(Count::span) {
            return Err(syn::Error::new(
                span,
                "count/count_with attribute not valid on scalar fields",
            ));
        }
        if let Some(token) = self.variable_size {
            return Err(syn::Error::new(token.span(), "not valid on scalar fields"));
        }

        Ok(ScalarField {
            name,
            typ,
            hidden: self.hidden,
        })
    }
}

fn expect_ident(meta: &syn::NestedMeta) -> Result<syn::Ident, syn::Error> {
    match meta {
        syn::NestedMeta::Meta(syn::Meta::Path(p)) if p.get_ident().is_some() => {
            Ok(p.get_ident().unwrap().clone())
        }
        _ => Err(syn::Error::new(meta.span(), "expected ident")),
    }
}

impl Count {
    fn span(&self) -> proc_macro2::Span {
        match self {
            Count::Field(ident) => ident.span(),
            Count::Function { fn_, .. } => fn_.span(),
        }
    }
}

impl Field {
    pub fn concrete_type_tokens(&self) -> proc_macro2::TokenStream {
        match &self {
            Self::Array(ArrayField {
                inner_typ,
                inner_lifetime,
                ..
            }) => match ScalarType::from_str(&inner_typ.to_string()).map(|s| s.raw_type_tokens()) {
                Ok(typ) => quote!([#typ]),
                Err(_) if *inner_lifetime => quote!([#inner_typ<'a>]),
                Err(_) => quote!([#inner_typ]),
            },
            Self::Scalar(ScalarField { typ, .. }) => typ.raw_type_tokens(),
        }
    }

    pub fn as_scalar(&self) -> Option<&ScalarField> {
        match self {
            Field::Array(_) => None,
            Field::Scalar(v) => Some(v),
        }
    }

    pub fn as_array(&self) -> Option<&ArrayField> {
        match self {
            Field::Array(v) => Some(v),
            Field::Scalar(_) => None,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Field::Array(_))
    }

    fn requires_lifetime(&self) -> bool {
        self.is_array()
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self, Field::Scalar(_))
    }
}

impl Item {
    fn validate(&self) -> Result<(), syn::Error> {
        let needs_lifetime = self.fields.iter().any(|x| x.requires_lifetime());
        if needs_lifetime && !self.lifetime {
            let msg = format!(
                "object containing array or offset requires lifetime param ({}<'a>)",
                self.name
            );
            return Err(syn::Error::new(self.name.span(), &msg));
        } else if !needs_lifetime && self.lifetime {
            return Err(syn::Error::new(
                self.name.span(),
                "only objects containing arrays or offsets require lifetime",
            ));
        }
        Ok(())
    }

    pub fn checkable_len(&self) -> usize {
        self.fields
            .iter()
            .filter_map(|fld| match fld {
                Field::Array(_) => None,
                Field::Scalar(val) => Some(val.typ.size()),
            })
            .sum()
    }
}

impl ScalarType {
    const fn size(self) -> usize {
        match self {
            ScalarType::I8 | ScalarType::U8 => 1,
            ScalarType::I16 | ScalarType::U16 | ScalarType::Offset16 | ScalarType::F2Dot14 => 2,
            ScalarType::U24 | ScalarType::Offset24 => 3,
            ScalarType::Fixed
            | ScalarType::Tag
            | ScalarType::U32
            | ScalarType::I32
            | ScalarType::Version16Dot16
            | ScalarType::Offset32 => 4,
            ScalarType::LongDateTime => 8,
        }
    }

    fn raw_type_tokens(&self) -> proc_macro2::TokenStream {
        match self {
            Self::I8 => quote!(::raw_types::Int8),
            Self::U8 => quote!(::raw_types::Uint8),
            Self::I16 => quote!(::raw_types::Int16),
            Self::U16 => quote!(::raw_types::Uint16),
            Self::U24 => quote!(::raw_types::Uint24),
            Self::I32 => quote!(::raw_types::Int32),
            Self::U32 => quote!(::raw_types::Uint32),
            Self::Fixed => quote!(::raw_types::Fixed),
            Self::F2Dot14 => quote!(::raw_types::F2Dot14),
            Self::LongDateTime => quote!(::raw_types::LongDateTime),
            Self::Offset16 => quote!(::raw_types::Offset16),
            Self::Offset24 => quote!(::raw_types::Offset24),
            Self::Offset32 => quote!(::raw_types::Offset32),
            Self::Tag => quote!(::raw_types::Tag),
            Self::Version16Dot16 => quote!(::raw_types::Version16Dot16),
        }
    }
}

impl Parse for ScalarType {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let name: syn::Ident = input
            .parse()
            .map_err(|e| syn::Error::new(e.span(), "expected scalar type"))?;
        let name_str = name.to_string();
        ScalarType::from_str(&name_str)
            .map_err(|_| syn::Error::new(name.span(), "Expected scalar type"))
    }
}

impl FromStr for ScalarType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Int8" => Ok(Self::I8),
            "Uint8" => Ok(Self::U8),
            "Int16" => Ok(Self::I16),
            "Uint16" => Ok(Self::U16),
            "Uint24" => Ok(Self::U24),
            "Int32" => Ok(Self::I32),
            "Uint32" => Ok(Self::U32),
            "Fixed" => Ok(Self::Fixed),
            "F2Dot14" => Ok(Self::F2Dot14),
            "LongDateTime" => Ok(Self::LongDateTime),
            "Offset16" => Ok(Self::Offset16),
            "Offset24" => Ok(Self::Offset24),
            "Offset32" => Ok(Self::Offset32),
            "Tag" => Ok(Self::Tag),
            "Version16Dot16" => Ok(Self::Version16Dot16),
            _ => Err(()),
        }
    }
}

fn get_optional_attributes(input: ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    let mut result = Vec::new();
    while input.lookahead1().peek(Token![#]) {
        result.extend(Attribute::parse_outer(input)?);
    }
    Ok(result)
}

/// Check that generic arguments are acceptable
///
/// They are acceptable if they are empty, or contain a single lifetime.
fn get_generics(input: ParseStream) -> Result<bool, syn::Error> {
    let generics = input.parse::<syn::Generics>()?;
    if generics.type_params().count() + generics.const_params().count() > 0 {
        return Err(syn::Error::new(
            generics.span(),
            "generics are not allowed in font tables",
        ));
    }
    if let Some(lifetime) = generics.lifetimes().nth(1) {
        return Err(syn::Error::new(
            lifetime.span(),
            "tables can contain at most a single lifetime",
        ));
    }

    let lifetime = generics.lifetimes().next();
    match lifetime {
        Some(ltime) => {
            if ltime.colon_token.is_some() || !ltime.attrs.is_empty() {
                let span = if ltime.colon_token.is_some() {
                    ltime.bounds.span()
                } else {
                    ltime.span()
                };
                Err(syn::Error::new(
                    span,
                    "only a single unbounded lifetime is allowed",
                ))
            } else {
                Ok(true)
            }
        }
        None => Ok(false),
    }
}
