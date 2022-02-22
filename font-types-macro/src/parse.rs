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
    RawEnum(RawEnum),
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

/// A raw c-style enum
pub struct RawEnum {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub repr: syn::Ident,
    pub variants: Vec<RawVariant>,
}

/// A raw scalar variant
pub struct RawVariant {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub value: syn::LitInt,
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
    pub inner_typ: syn::Path,
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
        let lifetime = validate_lifetime(input)?;
        let content;
        let _ = braced!(content in input);
        if enum_token.is_some() && attrs.repr.is_some() {
            let variants = Punctuated::<RawVariant, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            RawEnum::new(name, variants, attrs).map(Self::RawEnum)
        } else if enum_token.is_some() {
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
        let typ_lifetime = validate_lifetime(&content)?;
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

impl Parse for RawVariant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = VariantAttrs::parse(&attrs)?;
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![=]>()?;
        let value: syn::LitInt = input.parse()?;
        Ok(Self {
            docs: attrs.docs,
            name,
            value,
        })
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
            let typ = content.parse::<syn::Path>()?;
            let lifetime = ensure_single_lifetime(&typ)?;
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

    pub fn is_be_wrapper(&self) -> bool {
        matches!(self.typ.segments.last(), Some(seg) if seg.ident == "BigEndian")
    }

    /// The return type of a getter of this type.
    ///
    /// (this is about returning T for BigEndian<T>)
    pub fn cooked_type_tokens(&self) -> proc_macro2::TokenStream {
        let last = self.typ.segments.last().unwrap();
        if last.ident == "BigEndian" {
            let args = match &last.arguments {
                syn::PathArguments::AngleBracketed(args) => args,
                _ => panic!("BigEndian type should always have generic params"),
            };
            let last_arg = args.args.last().unwrap();
            if let syn::GenericArgument::Type(inner) = last_arg {
                return quote!(#inner);
            }
            panic!("failed to find BigEndian generic type");
        }

        let typ = &self.typ;
        quote!(#typ)
    }

    /// tokens for getting this field from a byte slice.
    ///
    /// This should return Option<Self>, and it will be unwrapped where it has been prechecked.
    pub fn getter_tokens(&self, bytes: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let typ = &self.typ;
        let convert_to_cooked = self.is_be_wrapper().then(|| quote!(.map(|x| x.get())));
        quote!(<#typ as zerocopy::FromBytes>::read_from(#bytes) #convert_to_cooked )
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

impl RawEnum {
    fn new(
        name: syn::Ident,
        variants: Vec<RawVariant>,
        attrs: ItemAttrs,
    ) -> Result<Self, syn::Error> {
        let repr = attrs.repr.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "raw enumerations require repr annotation (like: #[repr(u16)])",
            )
        })?;
        Ok(RawEnum {
            docs: attrs.docs,
            repr,
            variants,
            name,
        })
    }
}

fn get_optional_attributes(input: ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    let mut result = Vec::new();
    while input.lookahead1().peek(Token![#]) {
        result.extend(Attribute::parse_outer(input)?);
    }
    Ok(result)
}

fn ensure_single_lifetime(input: &syn::Path) -> Result<Option<syn::Lifetime>, syn::Error> {
    match input.segments.last().map(|seg| &seg.arguments) {
        Some(syn::PathArguments::AngleBracketed(args)) => {
            let mut iter = args.args.iter().filter_map(|arg| match arg {
                syn::GenericArgument::Lifetime(arg) => Some(arg),
                _ => None,
            });
            let result = iter.next();
            if let Some(extra_lifetime) = iter.next() {
                Err(syn::Error::new(
                    extra_lifetime.span(),
                    "at most a single lifetime \"'a\" is supported",
                ))
            } else {
                Ok(result.cloned())
            }
        }
        Some(syn::PathArguments::Parenthesized(args)) => Err(syn::Error::new(
            args.span(),
            "whatever this is trying to do, we definitely do not support it",
        )),
        None | Some(syn::PathArguments::None) => Ok(None),
    }
}

/// Ensure types have at most a single lifetime param, "'a".
fn validate_lifetime(input: ParseStream) -> Result<Option<syn::Lifetime>, syn::Error> {
    let generics = input.parse::<syn::Generics>()?;
    if generics.const_params().count() + generics.type_params().count() > 1 {
        return Err(syn::Error::new(
            generics.span(),
            "font types should contain at most a single lifetime",
        ));
    }
    if let Some(lifetime) = generics.lifetimes().nth(1) {
        return Err(syn::Error::new(
            lifetime.span(),
            "tables can contain at most a single lifetime",
        ));
    }

    let result = generics.lifetimes().next().map(|lt| &lt.lifetime).cloned();
    Ok(result)
}
