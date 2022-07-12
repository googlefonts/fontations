use quote::{quote, quote_spanned, ToTokens};
use syn::{
    braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token, Attribute, Token,
};

pub fn generate_parse_module(code: &str) -> Result<proc_macro2::TokenStream, syn::Error> {
    let items: Items = syn::parse_str(code)?;
    Ok(Default::default())
}

pub struct Items {
    //pub use_stmts: Vec<SimpleUse>,
    pub items: Vec<Item>,
}

pub enum Item {
    Table(Table),
    Record(Record),
    Format(TableFormat),
    RawEnum(RawEnum),
    Flags(BitFlags),
}

#[derive(Debug, Clone)]
pub(crate) struct Table {
    //pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub fields: Fields,
}

#[derive(Debug, Clone)]
pub(crate) struct Record {
    //pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub fields: Fields,
}

/// A table with a format; we generate an enum
#[derive(Debug, Clone)]
pub(crate) struct TableFormat {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub(crate) format: syn::Ident,
    pub(crate) variants: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub(crate) struct Variant {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub typ: syn::Ident,
}

#[derive(Debug, Clone)]
pub(crate) struct Fields {
    pub(crate) fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub(crate) struct Field {
    //pub(crate) docs: Vec<syn::Attribute>,
    //pub(crate) attrs: FieldAttrs,
    pub(crate) name: syn::Ident,
    pub(crate) typ: FieldType,
}

#[derive(Debug, Clone)]
pub(crate) struct FieldAttrs {}

#[derive(Debug, Clone)]
pub enum FieldType {
    Offset { typ: syn::Ident },
    Scalar { typ: syn::Ident },
    Other { typ: syn::Path },
    Array { inner_typ: Box<FieldType> },
}

/// A raw c-style enum
#[derive(Debug, Clone)]
pub(crate) struct RawEnum {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub(crate) typ: syn::Ident,
    pub(crate) variants: Vec<RawVariant>,
}

/// A raw scalar variant
#[derive(Debug, Clone)]
pub(crate) struct RawVariant {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub(crate) value: syn::LitInt,
}

/// A set of bit-flags
#[derive(Debug, Clone)]
pub(crate) struct BitFlags {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub(crate) typ: syn::Ident,
    pub(crate) variants: Vec<RawVariant>,
}

mod kw {
    syn::custom_keyword!(table);
    syn::custom_keyword!(record);
    syn::custom_keyword!(flags);
    syn::custom_keyword!(format);
}

impl Parse for Items {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        //let use_stmts = get_use_statements(input)?;
        let mut items = Vec::new();
        while !input.is_empty() {
            items.push(input.parse()?);
        }
        Ok(Self { items })
    }
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let peek = input.fork();
        // skip attributes
        while peek.lookahead1().peek(Token![#]) {
            Attribute::parse_outer(&peek)?;
        }

        let lookahead = peek.lookahead1();
        if lookahead.peek(kw::table) {
            Ok(Self::Table(input.parse()?))
        } else if lookahead.peek(kw::record) {
            Ok(Self::Record(input.parse()?))
        } else if lookahead.peek(kw::flags) {
            Ok(Self::Flags(input.parse()?))
        } else if lookahead.peek(kw::format) {
            Ok(Self::Format(input.parse()?))
        } else if lookahead.peek(Token![enum]) {
            Ok(Self::RawEnum(input.parse()?))
        } else {
            Err(syn::Error::new(
                input.span(),
                "expected one of 'table' 'record' 'flags' 'format' or 'enum'.",
            ))
        }
    }
}

impl Parse for Table {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attributes = get_optional_attributes(input)?;
        let _table = input.parse::<kw::table>()?;
        let name = input.parse::<syn::Ident>()?;

        let fields = input.parse()?;
        Ok(Table { name, fields })
    }
}

impl Parse for Record {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attributes = get_optional_attributes(input)?;
        let _kw = input.parse::<kw::record>()?;
        let name = input.parse::<syn::Ident>()?;

        let fields = input.parse()?;
        Ok(Record { name, fields })
    }
}

impl Parse for BitFlags {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let docs = get_optional_docs(input)?;
        let _kw = input.parse::<kw::flags>()?;
        let typ = input.parse::<syn::Ident>()?;
        validate_ident(&typ, &["u8", "u16"], "allowed bitflag types: u8, u16")?;
        let name = input.parse::<syn::Ident>()?;

        let content;
        let _ = braced!(content in input);
        let variants = Punctuated::<RawVariant, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();

        Ok(BitFlags {
            docs,
            name,
            typ,
            variants,
        })
    }
}

impl Parse for RawEnum {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let docs = get_optional_docs(input)?;
        let _kw = input.parse::<Token![enum]>()?;
        let typ = input.parse::<syn::Ident>()?;
        validate_ident(&typ, &["u8", "u16"], "allowed enum types: u8, u16")?;
        let name = input.parse::<syn::Ident>()?;
        let content;
        let _ = braced!(content in input);
        let variants = Punctuated::<RawVariant, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(RawEnum {
            docs,
            name,
            typ,
            variants,
        })
    }
}

impl Parse for TableFormat {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        //let attributes = get_optional_attributes(input)?;
        let docs = get_optional_docs(input)?;
        let _kw = input.parse::<kw::format>()?;
        let format: syn::Ident = input.parse()?;
        validate_ident(&format, &["u16"], "unexpected format type")?;
        let name = input.parse::<syn::Ident>()?;

        let content;
        let _ = braced!(content in input);
        let variants = Punctuated::<Variant, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();

        Ok(TableFormat {
            docs,
            format,
            name,
            variants,
        })
    }
}

impl Parse for Fields {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        let _ = braced!(content in input);
        let fields = Punctuated::<Field, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(Self { fields })
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _attrs = get_optional_attributes(input)?;
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![:]>()?;
        let typ = input.parse()?;
        Ok(Field { name, typ })
    }
}

impl Parse for FieldType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.lookahead1().peek(token::Bracket) {
            let content;
            bracketed!(content in input);
            let span = content.span();
            let inner_typ: FieldType = content.parse()?;
            if matches!(inner_typ, FieldType::Array { .. }) {
                return Err(syn::Error::new(span, "nested arrays are invalid"));
            }
            return Ok(FieldType::Array {
                inner_typ: Box::new(inner_typ),
            });
        }

        let path = input.parse::<syn::Path>()?;
        let last = path.segments.last().expect("do zero-length paths exist?");
        if last.ident != "BigEndian" {
            return Ok(FieldType::Other { typ: path.clone() });
        }

        let inner = get_single_generic_type_arg(&last.arguments).ok_or_else(|| {
            syn::Error::new(last.ident.span(), "expected single generic type argument")
        })?;
        let last = inner.segments.last().unwrap();
        if ["Offset16", "Offset24", "Offset32"].contains(&last.ident.to_string().as_str()) {
            Ok(FieldType::Offset {
                typ: last.ident.clone(),
            })
        } else if last.arguments.is_empty() {
            Ok(FieldType::Scalar {
                typ: last.ident.clone(),
            })
        } else {
            Err(syn::Error::new(last.span(), "unexpected arguments"))
        }
    }
}

impl Parse for RawVariant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let docs = get_optional_docs(input)?;
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![=]>()?;
        let value: syn::LitInt = input.parse()?;
        Ok(Self { docs, name, value })
    }
}

impl Parse for Variant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let docs = get_optional_docs(input)?;
        let name = input.parse::<syn::Ident>()?;
        let content;
        parenthesized!(content in input);
        let typ = content.parse::<syn::Ident>()?;
        Ok(Self { docs, name, typ })
    }
}

fn validate_ident(ident: &syn::Ident, expected: &[&str], error: &str) -> Result<(), syn::Error> {
    if !expected.iter().any(|exp| ident == exp) {
        return Err(syn::Error::new(ident.span(), error));
    }
    Ok(())
}

fn get_optional_attributes(input: ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    let mut result = Vec::new();
    while input.lookahead1().peek(Token![#]) {
        result.extend(Attribute::parse_outer(input)?);
    }
    Ok(result)
}

fn get_optional_docs(input: ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    let mut result = Vec::new();
    while input.lookahead1().peek(Token![#]) {
        result.extend(Attribute::parse_outer(input)?);
    }
    for attr in &result {
        if !attr.path.is_ident("doc") {
            return Err(syn::Error::new(attr.span(), "expected doc comment"));
        }
    }
    Ok(result)
}

fn get_single_generic_type_arg(input: &syn::PathArguments) -> Option<syn::Path> {
    match input {
        syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
            let arg = args.args.last().unwrap();
            if let syn::GenericArgument::Type(syn::Type::Path(path)) = arg {
                if path.qself.is_none() && path.path.segments.len() == 1 {
                    return Some(path.path.clone());
                }
            }
            None
        }
        _ => None,
    }
}
