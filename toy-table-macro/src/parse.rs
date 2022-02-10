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

pub struct Field {
    pub attrs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
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

pub enum Type {
    Single(Scalar),
    Array { typ: syn::Ident, lifetime: bool },
    // not sure we want to have full paths here? I think everything should need
    // to be in scope.
    //Path(syn::Path),
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
        let attrs = get_optional_attributes(&input)?;
        let name: syn::Ident = input.parse()?;
        let lifetime = get_generics(&input)?;
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
        let attrs = get_optional_attributes(&input)?;
        let name = input.parse()?;
        let _ = input.parse::<Token![:]>()?;
        let ty = input.parse()?;

        Ok(Field { attrs, name, ty })
    }
}

impl Field {
    pub fn concrete_type_tokens(&self) -> proc_macro2::TokenStream {
        match &self.ty {
            Type::Array { typ, lifetime } => {
                match Scalar::from_str(&typ.to_string()).map(|s| s.raw_type_tokens()) {
                    Ok(typ) => quote!([#typ]),
                    Err(_) if *lifetime => quote!([#typ<'a>]),
                    Err(_) => quote!([#typ]),
                }
            }
            Type::Single(scalar) => scalar.raw_type_tokens(),
        }
    }

    fn is_array(&self) -> bool {
        matches!(self.ty, Type::Array { .. })
    }

    fn requires_lifetime(&self) -> bool {
        matches!(
            self.ty,
            Type::Array { .. }
                | Type::Single(Scalar::Offset16 | Scalar::Offset24 | Scalar::Offset32)
        )
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self.ty, Type::Single(_))
    }
}

impl Parse for Type {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        if input.lookahead1().peek(token::Bracket) {
            let content;
            bracketed!(content in input);
            let typ = content.parse::<syn::Ident>()?;
            let lifetime = get_generics(&&content)?;

            return Ok(Type::Array { typ, lifetime });
        }

        input.parse().map(Type::Single)
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
            .filter_map(|fld| {
                if let Type::Single(scalar) = fld.ty {
                    Some(scalar.size())
                } else {
                    None
                }
            })
            .sum()
    }
}

impl Scalar {
    const fn size(self) -> usize {
        match self {
            Scalar::I8 | Scalar::U8 => 1,
            Scalar::I16 | Scalar::U16 | Scalar::Offset16 | Scalar::F2Dot14 => 2,
            Scalar::U24 | Scalar::Offset24 => 3,
            Scalar::Fixed
            | Scalar::Tag
            | Scalar::U32
            | Scalar::I32
            | Scalar::Version16Dot16
            | Scalar::Offset32 => 4,
            Scalar::LongDateTime => 8,
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

impl Parse for Scalar {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let name: syn::Ident = input
            .parse()
            .map_err(|e| syn::Error::new(e.span(), "expected scalar type"))?;
        let name_str = name.to_string();
        Scalar::from_str(&name_str)
            .map_err(|_| syn::Error::new(name.span(), "Expected scalar type"))
    }
}

impl FromStr for Scalar {
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

fn get_optional_attributes(input: &ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    Ok(input
        .lookahead1()
        .peek(Token![#])
        .then(|| Attribute::parse_outer(input))
        .transpose()?
        .unwrap_or_default())
}

/// Check that generic arguments are acceptable
///
/// They are acceptable if they are empty, or contain a single lifetime.
fn get_generics(input: &ParseStream) -> Result<bool, syn::Error> {
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
