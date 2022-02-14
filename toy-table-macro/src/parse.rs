#![allow(dead_code)]

use std::str::FromStr;

use quote::quote;
use syn::{
    braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token, Attribute, Token,
};

mod attrs;

pub use attrs::Count;
use attrs::{FieldAttrs, VariantAttrs};

use self::attrs::ItemAttrs;

pub struct Items(Vec<Item>);

pub enum Item {
    Single(SingleItem),
    Group(ItemGroup),
}

/// A single concrete object, such as a particular table version or record format.
pub struct SingleItem {
    pub lifetime: Option<syn::Lifetime>,
    pub name: syn::Ident,
    pub fields: Vec<Field>,
}

/// A group of items that can exist in the same location, such as tables
/// with multiple versions.
pub struct ItemGroup {
    pub name: syn::Ident,
    pub lifetime: Option<syn::Lifetime>,
    pub format_typ: syn::Ident,
    pub variants: Vec<Variant>,
}

pub struct Variant {
    pub name: syn::Ident,
    pub version: syn::Path,
    pub typ: syn::Ident,
    pub typ_lifetime: Option<syn::Lifetime>,
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
    pub inner_lifetime: Option<syn::Lifetime>,
    pub count: Count,
    pub variable_size: Option<syn::Path>,
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
        let attrs = ItemAttrs::parse(&attrs)?;
        let enum_token = input
            .peek(Token![enum])
            .then(|| input.parse::<Token![enum]>())
            .transpose()?;
        let name: syn::Ident = input.parse()?;
        let lifetime = get_generics(input)?;
        let content;
        let _ = braced!(content in input);
        if let Some(_token) = enum_token {
            let variants = Punctuated::<Variant, Token![,]>::parse_terminated(&content)?;
            let variants = variants.into_iter().collect();
            ItemGroup::new(name, lifetime, variants, attrs).map(Self::Group)
        } else {
            let fields = Punctuated::<Field, Token![,]>::parse_terminated(&content)?;
            let fields = fields.into_iter().collect();
            let item = SingleItem {
                lifetime,
                //attrs,
                name,
                fields,
            };
            item.validate()?;
            Ok(Self::Single(item))
        }
    }
}

impl Parse for Variant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = VariantAttrs::parse(&attrs)?;
        let name = input.parse::<syn::Ident>()?;
        let content;
        parenthesized!(content in input);
        let typ = content.parse::<syn::Ident>()?;
        let typ_lifetime = get_generics(&content)?;
        if let Some(version) = attrs.version {
            Ok(Self {
                name,
                version,
                typ,
                typ_lifetime,
            })
        } else {
            Err(syn::Error::new(
                name.span(),
                "all variants require #[version(..)] attribute",
            ))
        }
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = FieldAttrs::parse(&attrs)?;
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

impl Field {
    pub fn concrete_type_tokens(&self) -> proc_macro2::TokenStream {
        match &self {
            Self::Array(ArrayField {
                inner_typ,
                inner_lifetime,
                ..
            }) => match ScalarType::from_str(&inner_typ.to_string()).map(|s| s.raw_type_tokens()) {
                Ok(typ) => quote!([#typ]),
                Err(_) if inner_lifetime.is_some() => quote!([#inner_typ<'a>]),
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

impl SingleItem {
    fn validate(&self) -> Result<(), syn::Error> {
        let needs_lifetime = self.fields.iter().any(|x| x.requires_lifetime());
        if needs_lifetime && self.lifetime.is_none() {
            let msg = format!(
                "object containing array or offset requires lifetime param ({}<'a>)",
                self.name
            );
            return Err(syn::Error::new(self.name.span(), &msg));
        } else if !needs_lifetime && self.lifetime.is_some() {
            return Err(syn::Error::new(
                self.lifetime.as_ref().unwrap().span(),
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

impl ItemGroup {
    fn new(
        name: syn::Ident,
        lifetime: Option<syn::Lifetime>,
        variants: Vec<Variant>,
        attrs: ItemAttrs,
    ) -> Result<Self, syn::Error> {
        if let Some(format_typ) = attrs.format {
            Ok(Self {
                name,
                lifetime,
                variants,
                format_typ,
            })
        } else {
            Err(syn::Error::new(
                name.span(),
                "all enum groups require #[format(..)] attribute",
            ))
        }
    }
}
impl ScalarType {
    pub const fn size(self) -> usize {
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

    pub fn raw_type_tokens(&self) -> proc_macro2::TokenStream {
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
fn get_generics(input: ParseStream) -> Result<Option<syn::Lifetime>, syn::Error> {
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
                Ok(Some(ltime.lifetime.clone()))
            }
        }
        None => Ok(None),
    }
}
