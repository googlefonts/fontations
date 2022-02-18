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
    pub docs: Vec<syn::Attribute>,
    pub lifetime: Option<syn::Lifetime>,
    pub name: syn::Ident,
    pub fields: Vec<Field>,
}

/// A group of items that can exist in the same location, such as tables
/// with multiple versions.
pub struct ItemGroup {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub lifetime: Option<syn::Lifetime>,
    pub format_typ: syn::Ident,
    pub variants: Vec<Variant>,
}

pub struct Variant {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub version: syn::Path,
    pub typ: syn::Ident,
    pub typ_lifetime: Option<syn::Lifetime>,
}

pub enum Field {
    Single(SingleField),
    Array(ArrayField),
}

pub struct SingleField {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub typ: syn::Path,
    pub hidden: Option<syn::Path>,
}

pub struct ArrayField {
    pub docs: Vec<syn::Attribute>,
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
                docs: attrs.docs,
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
                docs: attrs.docs,
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
            attrs.into_single(name, input.parse()?).map(Field::Single)
        }
    }
}

impl Field {
    pub fn name(&self) -> &syn::Ident {
        match self {
            Field::Array(v) => &v.name,
            Field::Single(v) => &v.name,
        }
    }

    pub fn as_single(&self) -> Option<&SingleField> {
        match self {
            Field::Array(_) => None,
            Field::Single(v) => Some(v),
        }
    }

    pub fn as_array(&self) -> Option<&ArrayField> {
        match self {
            Field::Array(v) => Some(v),
            Field::Single(_) => None,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Field::Array(_))
    }

    fn requires_lifetime(&self) -> bool {
        self.is_array()
    }

    pub fn is_single(&self) -> bool {
        matches!(self, Field::Single(_))
    }

    pub fn docs(&self) -> &[syn::Attribute] {
        match self {
            Field::Array(v) => &v.docs,
            Field::Single(v) => &v.docs,
        }
    }
}

impl SingleField {
    /// tokens representing the length of this field in raw bytes
    pub fn len_tokens(&self) -> proc_macro2::TokenStream {
        let typ = &self.typ;
        quote!(std::mem::size_of::<#typ>())
        //quote!(std::mem::size_of::<<#typ as ::raw_types::FontType>::Raw>())
    }

    pub fn raw_type_tokens(&self) -> proc_macro2::TokenStream {
        let typ = &self.typ;
        quote!(#typ)
        //quote!(<#typ as ::raw_types::FontType::Raw>::Raw)
    }

    /// tokens for getting this field from a byte slice.
    ///
    /// This should return Option<Self>, and it will be unwrapped where it has been prechecked.
    pub fn getter_tokens(&self, bytes: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let typ = &self.typ;
        quote!(<#typ as zerocopy::FromBytes>::read_from(#bytes))
    }
}

impl SingleItem {
    fn validate(&self) -> Result<(), syn::Error> {
        // check for lifetime
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

        let split_pos = self
            .fields
            .iter()
            .position(|x| x.is_array())
            .unwrap_or_else(|| self.fields.len());

        let valid_input_fields = &self.fields[..split_pos];
        // check that fields are known & are scalar
        for ident in self
            .fields
            .iter()
            .filter_map(Field::as_array)
            .flat_map(|x| x.count.iter_input_fields())
        {
            if !valid_input_fields.iter().any(|x| x.name() == ident) {
                return Err(syn::Error::new(ident.span(), "unknown field"));
            }
        }
        Ok(())
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
                docs: attrs.docs,
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
