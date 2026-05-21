//! raw parsing code

mod attrs;
mod fields;
pub(crate) use attrs::*;
pub(crate) use fields::*;

use std::{backtrace::Backtrace, collections::HashMap, fmt::Display};

use indexmap::IndexMap;
use log::{debug, trace};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Token,
};

use crate::Phase;

#[derive(Debug)]
pub(crate) struct Items {
    pub(crate) parse_module_path: syn::Path,
    /// Whether the `#![sanitize]` module attribute is present,
    /// opting this module into `Sanitize` trait codegen.
    pub(crate) sanitize: bool,
    // we use an IndexMap so that we generate code in the same order as items
    // are declared in the input file.
    items: IndexMap<syn::Ident, Item>,
}

#[derive(Debug, Clone)]
pub(crate) enum Item {
    Table(Table),
    Record(Record),
    Format(TableFormat),
    GenericGroup(GenericGroup),
    RawEnum(RawEnum),
    Flags(BitFlags),
    Extern(Extern),
}

#[derive(Debug, Clone)]
pub(crate) struct Table {
    pub(crate) attrs: TableAttrs,
    pub(crate) name: syn::Ident,
    pub(crate) fields: Fields,
}

impl Table {
    // here for visibility reasons
    pub(crate) fn raw_name(&self) -> &syn::Ident {
        &self.name
    }

    /// Returns the table's format value if it has a `#[format(N)]` field.
    pub(crate) fn format_value_and_width(&self) -> Option<(u32, u8)> {
        let fld = self.fields.iter().find(|fld| fld.attrs.format.is_some())?;
        let format = fld.attrs.format.as_ref().unwrap().base10_parse().ok()?;
        let fld_tokens = fld.typ.cooked_type_tokens();
        let width = if fld_tokens == "u8" {
            1
        } else if fld_tokens == "u16" {
            2
        } else {
            panic!("only expect format fields to be u8 or u16");
        };
        Some((format, width))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub(crate) attrs: TableAttrs,
    pub(crate) lifetime: Option<TokenStream>,
    pub(crate) name: syn::Ident,
    pub(crate) fields: Fields,
}

/// A table with a format; we generate an enum
#[derive(Debug, Clone)]
pub(crate) struct TableFormat {
    pub(crate) attrs: TableAttrs,
    pub(crate) name: syn::Ident,
    pub(crate) format: syn::Ident,
    pub(crate) format_offset: Option<syn::LitInt>,
    pub(crate) variants: Vec<FormatVariant>,
}

#[derive(Debug, Clone)]
pub(crate) struct FormatVariant {
    pub(crate) attrs: VariantAttrs,
    pub(crate) name: syn::Ident,
    typ: syn::Ident,
}

/// Generates an enum where each variant has a different generic param to a single type.
///
/// This is used in GPOS/GSUB, allowing us to provide more type information
/// to lookups.
#[derive(Debug, Clone)]
pub(crate) struct GenericGroup {
    pub(crate) attrs: TableAttrs,
    pub(crate) name: syn::Ident,
    /// the inner type, which must accept a generic parameter
    pub(crate) inner_type: syn::Ident,
    pub(crate) variants: Vec<GroupVariant>,
}

#[derive(Debug, Clone)]
pub(crate) struct GroupVariant {
    pub(crate) type_id: syn::LitInt,
    pub(crate) name: syn::Ident,
    pub(crate) typ: syn::Ident,
}

impl FormatVariant {
    pub(crate) fn type_name(&self) -> &syn::Ident {
        &self.typ
    }
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
    pub(crate) attrs: EnumVariantAttrs,
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

#[derive(Debug, Clone)]
pub(crate) enum ExternType {
    Scalar,
    Record,
}

/// A scalar or record that the codegen user must define themselves
#[derive(Debug, Clone)]
pub(crate) struct Extern {
    pub(crate) name: syn::Ident,
    pub(crate) typ: ExternType,
}

mod kw {
    syn::custom_keyword!(table);
    syn::custom_keyword!(record);
    syn::custom_keyword!(flags);
    syn::custom_keyword!(format);
    syn::custom_keyword!(group);
    syn::custom_keyword!(scalar);
}

impl Parse for Items {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let mut items = IndexMap::new();
        let (parse_module_path, sanitize) = get_module_attrs(input)?;
        while !input.is_empty() {
            let item = input.parse::<Item>()?;
            items.insert(item.name().clone(), item);
        }
        Ok(Self {
            items,
            parse_module_path,
            sanitize,
        })
    }
}

/// Parse module-level inner attributes.
///
/// Required: `#![parse_module(read_fonts::tables::foo)]`
/// Optional: `#![sanitize]` — opts this module into `Sanitize` trait codegen.
fn get_module_attrs(input: ParseStream) -> syn::Result<(syn::Path, bool)> {
    let attrs = input.call(Attribute::parse_inner)?;

    let mut parse_module_path = None;
    let mut sanitize = false;

    for attr in &attrs {
        if attr.path().is_ident("parse_module") {
            parse_module_path = Some(attr.parse_args()?);
        } else if attr.path().is_ident("sanitize") {
            sanitize = true;
        } else {
            return Err(logged_syn_error(
                attr.span(),
                "unexpected attribute; expected `parse_module` or `sanitize`",
            ));
        }
    }

    let parse_module_path = parse_module_path.ok_or_else(|| {
        logged_syn_error(Span::call_site(), "expected #![parse_module(..)] attribute")
    })?;

    Ok((parse_module_path, sanitize))
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
        } else if lookahead.peek(kw::group) {
            Ok(Self::GenericGroup(input.parse()?))
        } else if lookahead.peek(Token![enum]) {
            Ok(Self::RawEnum(input.parse()?))
        } else if lookahead.peek(Token![extern]) {
            Ok(Self::Extern(input.parse()?))
        } else {
            Err(logged_syn_error(
                input.span(),
                "expected one of 'table' 'record' 'flags' 'format' 'enum', 'extern', or 'group'.",
            ))
        }
    }
}

impl Parse for Table {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs: TableAttrs = input.parse()?;
        let _table = input.parse::<kw::table>()?;
        let name = input.parse::<syn::Ident>()?;

        let mut fields: Fields = input.parse()?;
        fields.read_args = attrs.read_args.clone().map(|attrs| attrs.attr);
        Ok(Table {
            attrs,
            name,
            fields,
        })
    }
}

impl Parse for Record {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs: TableAttrs = input.parse()?;
        let _kw = input.parse::<kw::record>()?;
        let name = input.parse::<syn::Ident>()?;
        let lifetime = input
            .peek(Token![<])
            .then(|| {
                input.parse::<Token![<]>()?;
                input.parse::<syn::Lifetime>()?;
                input.parse::<Token![>]>().map(|_| quote!(<'a>))
            })
            .transpose()?;

        let mut fields: Fields = input.parse()?;
        fields.read_args = attrs.read_args.clone().map(|attrs| attrs.attr);
        Ok(Record {
            attrs,
            lifetime,
            name,
            fields,
        })
    }
}

impl Parse for BitFlags {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let docs = get_optional_docs(input)?;
        let _kw = input.parse::<kw::flags>()?;
        let typ = input.parse::<syn::Ident>()?;
        validate_ident(
            &typ,
            &["u8", "u16", "u32"],
            "allowed bitflag types: u8, u16, u32",
        )?;
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

impl Parse for Extern {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _kw = input.parse::<Token![extern]>()?;
        let lookahead = input.lookahead1();
        let typ = if lookahead.peek(kw::scalar) {
            input.parse::<kw::scalar>()?;
            ExternType::Scalar
        } else if lookahead.peek(kw::record) {
            input.parse::<kw::record>()?;
            ExternType::Record
        } else {
            return Err(lookahead.error());
        };
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![;]>();
        Ok(Extern { name, typ })
    }
}

impl Parse for TableFormat {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs: TableAttrs = input.parse()?;
        let _kw = input.parse::<kw::format>()?;
        let format: syn::Ident = input.parse()?;
        let format_offset = if input.peek(Token![@]) {
            input.parse::<Token![@]>()?;
            let offset = input.parse::<syn::LitInt>()?;
            if offset.base10_parse::<u16>().is_err() {
                return Err(syn::Error::new(
                    offset.span(),
                    "value must be an unsigned integer",
                ));
            }
            Some(offset)
        } else {
            None
        };
        validate_ident(
            &format,
            &["u8", "u16", "i16", "DeltaFormat"],
            "unexpected format type",
        )?;
        let name = input.parse::<syn::Ident>()?;

        let content;
        let _ = braced!(content in input);
        let variants = Punctuated::<FormatVariant, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();

        Ok(TableFormat {
            attrs,
            format,
            name,
            variants,
            format_offset,
        })
    }
}

impl Parse for GenericGroup {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.parse::<TableAttrs>()?;
        let _kw = input.parse::<kw::group>()?;
        let name = input.parse()?;
        let content;
        let _ = parenthesized!(content in input);
        let inner_type = content.parse()?;
        let content;
        let _ = braced!(content in input);
        let variants = Punctuated::<GroupVariant, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(GenericGroup {
            attrs,
            name,
            inner_type,
            variants,
        })
    }
}

impl Parse for GroupVariant {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let type_id = input.parse()?;
        input.parse::<Token![=>]>()?;
        let name = input.parse()?;
        let content;
        let _ = parenthesized!(content in input);
        let typ = content.parse()?;
        Ok(GroupVariant { type_id, name, typ })
    }
}

impl Parse for RawVariant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = input.parse()?;
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![=]>()?;
        let value: syn::LitInt = input.parse()?;
        Ok(Self { attrs, name, value })
    }
}

impl Parse for FormatVariant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = input.parse()?;
        let name = input.parse::<syn::Ident>()?;
        let content;
        parenthesized!(content in input);
        let typ = content.parse::<syn::Ident>()?;
        Ok(Self { attrs, name, typ })
    }
}

impl Items {
    pub(crate) fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        for item in self.iter() {
            item.sanity_check(phase)?;
        }
        Ok(())
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Item> + '_ {
        self.items.values()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Item> + '_ {
        self.items.values_mut()
    }

    pub(crate) fn get(&self, name: &syn::Ident) -> Option<&Item> {
        self.items.get(name)
    }

    pub(crate) fn resolve_pending(&mut self) -> Result<(), syn::Error> {
        // We should know what some stuff is now
        // In theory we could repeat resolution until we succeed or stop learning
        // but I don't think ever need that currently
        let known = self.build_type_map();
        known.iter().for_each(|(k, v)| trace!("{} => {:?}", k, v));

        // Try to resolve everything pending against the known world
        for item in self.iter_mut() {
            let fields = match item {
                Item::Record(item) => &mut item.fields.fields,
                Item::Table(item) => &mut item.fields.fields,
                _ => continue,
            };
            for field in fields.iter_mut() {
                fields::resolve_field(&known, field)?;
            }
            // Mark fields as validated_at_parse if they have a known, fixed size
            // and appear before any variable-length fields (arrays).
            // Arrays return Some(empty token), so we stop when we see one.
            for field in fields
                .iter_mut()
                .take_while(|fld| fld.known_min_size_stmt().is_some_and(|t| !t.is_empty()))
            {
                field.validated_at_parse = true;
            }
        }

        Ok(())
    }

    // we return a new structure instead of resolving against self because
    // resolution involves mutable access to self.
    fn build_type_map(&self) -> HashMap<syn::Ident, FieldType> {
        self.items
            .iter()
            .filter_map(|(key, value)| {
                let value = match value {
                    Item::Table(_)
                    | Item::Record(_)
                    | Item::Extern(Extern {
                        typ: ExternType::Record,
                        ..
                    }) => FieldType::Struct {
                        typ: value.name().clone(),
                    },
                    Item::Flags(_)
                    | Item::RawEnum(_)
                    | Item::Extern(Extern {
                        typ: ExternType::Scalar,
                        ..
                    }) => FieldType::Scalar {
                        typ: value.name().clone(),
                    },
                    Item::Format(_) | Item::GenericGroup(_) => return None,
                };
                Some((key.clone(), value))
            })
            .collect()
    }
}

impl Item {
    fn name(&self) -> &syn::Ident {
        match self {
            Item::Table(table) => &table.name,
            Item::Record(record) => &record.name,
            Item::Format(group) => &group.name,
            Item::GenericGroup(group) => &group.name,
            Item::RawEnum(item) => &item.name,
            Item::Flags(item) => &item.name,
            Item::Extern(item) => &item.name,
        }
    }

    fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        match self {
            Item::Table(item) => item.sanity_check(phase),
            Item::Record(item) => item.sanity_check(phase),
            Item::Format(_) => Ok(()),
            Item::RawEnum(item) => item.sanity_check(phase),
            Item::Flags(_) => Ok(()),
            Item::GenericGroup(_) => Ok(()),
            Item::Extern(..) => Ok(()),
        }
    }
}

fn validate_ident(ident: &syn::Ident, expected: &[&str], error: &str) -> Result<(), syn::Error> {
    if !expected.iter().any(|exp| ident == exp) {
        return Err(logged_syn_error(ident.span(), error));
    }
    Ok(())
}

fn get_optional_docs(input: ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    let mut result = Vec::new();
    while input.lookahead1().peek(Token![#]) {
        result.extend(Attribute::parse_outer(input)?);
    }
    for attr in &result {
        if !attr.path().is_ident("doc") {
            return Err(logged_syn_error(attr.span(), "expected doc comment"));
        }
    }
    Ok(result)
}

pub(crate) fn logged_syn_error<T: Display>(span: Span, message: T) -> syn::Error {
    debug!("{}", Backtrace::capture());
    syn::Error::new(span, message)
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn parse_inline_expr_simple() {
        let s = "div_me($hi * 5)";
        let inline = syn::parse_str::<InlineExpr>(s).unwrap();
        assert_eq!(inline.referenced_fields.len(), 1);
        assert_eq!(
            inline.expr.into_token_stream().to_string(),
            "div_me (hi * 5)"
        );
    }

    #[test]
    fn parse_inline_expr_dedup() {
        let s = "div_me($hi * 5 + $hi)";
        let inline = syn::parse_str::<InlineExpr>(s).unwrap();
        assert_eq!(inline.referenced_fields.len(), 1);
        assert_eq!(
            inline.expr.into_token_stream().to_string(),
            "div_me (hi * 5 + hi)"
        );
    }

    fn parse_count(s: &str) -> Result<Count, syn::Error> {
        syn::parse_str(s)
    }

    #[test]
    fn test_count_attr() {
        assert!(matches!(
            parse_count("$hello"),
            Ok(Count::SingleArg(CountArg::Field(_)))
        ));
        assert!(matches!(
            parse_count("5"),
            Ok(Count::SingleArg(CountArg::Literal(_)))
        ));

        assert!(parse_count("hello").is_err());
        assert!(parse_count("$5").is_err());
        assert!(parse_count("5 - 2 as usize").is_err());

        assert!(matches!(
            parse_count("subtract(5, 2)"),
            Ok(Count::Complicated {
                xform: CountTransform::Sub,
                ..
            })
        ));

        assert!(parse_count("sub(5, 2)").is_err());
        assert!(parse_count("subtract(5)").is_err());
    }

    #[test]
    fn parse_version() {
        fn parse(s: &str) -> Result<VersionSpec, syn::Error> {
            syn::parse_str(s)
        }

        assert!(parse("32").unwrap().minor.is_none());
        assert_eq!(parse("32").unwrap().major, 32);
        assert!(parse("ab").is_err());
        assert!(parse("MajorMinor::VERSION_1_0").is_err());
        assert!(parse("1.3").unwrap().minor.is_some());
        assert!(parse("1.2.3").is_err());
        assert!(parse("1.'b'").is_err());
    }

    fn parse_format_group(s: &str) -> Result<TableFormat, syn::Error> {
        syn::parse_str(s)
    }

    #[test]
    fn parse_format_group_basic() {
        let s = "format u16 MyThing {
            FormatOne(SomeTable),
        }";
        let parsed = parse_format_group(s).unwrap();
        assert_eq!(parsed.format.to_string(), "u16");
        assert!(parsed.format_offset.is_none());
    }

    // just a sanity check
    #[test]
    fn parse_format_group_not_a_known_format() {
        let s = "format Fixed MyThing {
            FormatOne(SomeTable),
        }";
        assert!(parse_format_group(s).is_err());
    }

    #[test]
    fn parse_format_group_with_format_offset() {
        let s = "format u16@4 MyThing {
            FormatOne(SomeTable),
        }";

        let parsed = parse_format_group(s).unwrap();
        assert!(parsed.format_offset.is_some());
        assert_eq!(
            parsed.format_offset.unwrap().base10_parse::<u16>().ok(),
            Some(4)
        );
    }

    #[test]
    #[should_panic(expected = "must be an unsigned")]
    fn parse_format_group_with_negative_format_offset() {
        let s = "format u16@-4 MyThing {
            FormatOne(SomeTable),
        }";

        parse_format_group(s).unwrap();
    }

    #[test]
    fn parse_tag_attr() {
        let input: Table = parse_quote! {
            #[tag = "hilo"]
            table HiMom {}
        };

        assert_eq!(input.attrs.tag.unwrap().attr.value(), "hilo");
    }
}
