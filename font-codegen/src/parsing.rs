//! raw parsing code

use std::{
    backtrace::Backtrace, collections::HashMap, fmt::Display, hash::Hash, ops::Deref, str::FromStr,
};

use font_types::Tag;
use indexmap::IndexMap;
use log::{debug, trace};
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use regex::Captures;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Token,
};

#[derive(Debug)]
pub(crate) struct Items {
    pub(crate) parse_module_path: syn::Path,
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

#[derive(Debug, Copy, Clone)]
pub(crate) enum Phase {
    Parse,
    Analysis,
}

#[derive(Debug, Clone)]
pub(crate) struct Table {
    pub(crate) attrs: TableAttrs,
    name: syn::Ident,
    pub(crate) fields: Fields,
}

impl Table {
    // here for visibility reasons
    pub(crate) fn raw_name(&self) -> &syn::Ident {
        &self.name
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct TableAttrs {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) skip_font_write: Option<syn::Path>,
    pub(crate) skip_from_obj: Option<syn::Path>,
    pub(crate) skip_constructor: Option<syn::Path>,
    pub(crate) read_args: Option<Attr<TableReadArgs>>,
    pub(crate) generic_offset: Option<Attr<syn::Ident>>,
    pub(crate) tag: Option<Attr<syn::LitStr>>,
    pub(crate) write_only: Option<syn::Path>,
    /// Custom validation behaviour, must be a fn(&self, &mut ValidationCtx) for the type
    pub(crate) validate: Option<Attr<syn::Ident>>,
}

#[derive(Debug, Clone)]
pub(crate) struct TableReadArgs {
    pub(crate) args: Vec<TableReadArg>,
}

#[derive(Debug, Clone)]
pub(crate) struct TableReadArg {
    pub(crate) ident: syn::Ident,
    pub(crate) typ: syn::Ident,
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
    /// The field on the inner type that determines the type of the generic param
    pub(crate) inner_field: syn::Ident,
    pub(crate) variants: Vec<GroupVariant>,
}

#[derive(Debug, Clone)]
pub(crate) struct GroupVariant {
    pub(crate) type_id: syn::LitInt,
    pub(crate) name: syn::Ident,
    pub(crate) typ: syn::Ident,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VariantAttrs {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) match_stmt: Option<Attr<InlineExpr>>,
    pub(crate) write_only: Option<syn::Path>,
}

impl FormatVariant {
    pub(crate) fn marker_name(&self) -> syn::Ident {
        quote::format_ident!("{}Marker", &self.typ)
    }

    pub(crate) fn type_name(&self) -> &syn::Ident {
        &self.typ
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Fields {
    // not parsed, but set when the table/record is parsed
    pub(crate) read_args: Option<TableReadArgs>,
    pub(crate) fields: Vec<Field>,
    pub(crate) referenced_fields: ReferencedFields,
}

#[derive(Debug, Clone)]
pub(crate) struct ReferencedFields(HashMap<syn::Ident, NeededWhen>);

#[derive(Debug, Clone, Copy)]
pub(crate) enum NeededWhen {
    Parse,
    Runtime,
    Both,
}

#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub(crate) attrs: FieldAttrs,
    pub(crate) name: syn::Ident,
    pub(crate) typ: FieldType,
    /// `true` if this field is required to be read in order to parse subsequent
    /// fields.
    ///
    /// For instance: in a versioned table, the version must be read to determine
    /// whether to expect version-dependent fields.
    pub(crate) read_at_parse_time: bool,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct FieldAttrs {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) nullable: Option<syn::Path>,
    pub(crate) conditional: Option<Attr<Condition>>,
    pub(crate) skip_getter: Option<syn::Path>,
    /// specify that an offset getter has a custom impl
    pub(crate) offset_getter: Option<Attr<syn::Ident>>,
    /// optionally a method on the parent type used to generate the offset data
    /// source for this item.
    pub(crate) offset_data: Option<Attr<syn::Ident>>,
    /// If present, argument is an expression that evaluates to a u32, and is
    /// used to adjust the write position of offsets.
    //TODO: this could maybe be combined with offset_data?
    pub(crate) offset_adjustment: Option<Attr<InlineExpr>>,
    pub(crate) version: Option<syn::Path>,
    pub(crate) format: Option<Attr<syn::LitInt>>,
    pub(crate) count: Option<Attr<Count>>,
    pub(crate) compile: Option<Attr<CustomCompile>>,
    pub(crate) compile_with: Option<Attr<syn::Ident>>,
    pub(crate) default: Option<Attr<syn::Expr>>,
    pub(crate) compile_type: Option<Attr<syn::Type>>,
    pub(crate) read_with_args: Option<Attr<FieldReadArgs>>,
    pub(crate) read_offset_args: Option<Attr<FieldReadArgs>>,
    /// If present, a custom method that returns a FieldType for this field,
    /// during traversal.
    pub(crate) traverse_with: Option<Attr<syn::Ident>>,
    pub(crate) to_owned: Option<Attr<InlineExpr>>,
    /// Custom validation behaviour
    pub(crate) validate: Option<Attr<FieldValidation>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Attr<T> {
    pub(crate) name: syn::Ident,
    pub(crate) attr: T,
}

impl<T> Attr<T> {
    fn new(name: syn::Ident, attr: T) -> Self {
        Attr { name, attr }
    }

    pub(crate) fn span(&self) -> Span {
        self.name.span()
    }
}

impl<T> std::ops::Deref for Attr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.attr
    }
}

impl<T: ToTokens> ToTokens for Attr<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.attr.to_tokens(tokens)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FieldReadArgs {
    pub(crate) inputs: Vec<syn::Ident>,
}

#[derive(Clone, Debug)]
pub(crate) enum Condition {
    SinceVersion(VersionSpec),
    IfFlag { field: syn::Ident, flag: syn::Path },
    IfCond { xform: IfTransform },
}

#[derive(Clone, Debug)]
pub(crate) enum IfTransform {
    /// any_flag(field, flag_a, ...):
    ///
    /// Evaluates to true if field has at least one of the input flags set.
    AnyFlag(syn::Ident, Vec<syn::Path>),
}

enum IfArg {
    Field(syn::Ident),
    Path(syn::Path),
}

#[derive(Clone, Debug)]
pub(crate) struct VersionSpec {
    major: u16,
    minor: Option<u16>,
}

/// Annotations for how to calculate the count of an array.
///
/// ```no_compile
/// #[count(1)] #[count(..)] #[count($hi)] // simple
/// #[count(subtract($field, 1))] // complex
/// ```
#[derive(Clone, Debug)]
pub(crate) enum Count {
    // the field isn't used, but it's nice to hold onto if we want to print errors
    // in the future
    #[allow(dead_code)]
    All(syn::token::DotDot),
    SingleArg(CountArg),
    Complicated {
        args: Vec<CountArg>,
        xform: CountTransform,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum CountArg {
    Field(syn::Ident),
    Literal(syn::LitInt),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CountTransform {
    /// requires exactly two args, defined as $arg1 - $arg2
    Sub,
    /// requires exactly two args, defined as $arg1 + $arg2
    Add,
    /// requires exactly three args, defined as ($arg1 + $arg2) * $arg3
    AddMul,
    /// requires exactly three args, defined as ($arg1 * $arg2) + $arg3
    MulAdd,
    /// requires exactly one arg. defined as $arg1 / 2
    Half,
    DeltaValueCount,
    DeltaSetIndexData,
    /// three args: the axis count, the tuple index, and a constant on that index
    TupleLen,
    /// only ItemVariationStore: requires item_count, word_delta_count, region_index_count
    ItemVariationDataLen,
    /// Number of bytes to hold a bitmap of N items
    BitmapLen,
    /// Number of bytes to hold a bitmap with max value N
    MaxValueBitmapLen,
    /// requires exactly two args, defined as $arg1 - $arg2 + 2
    SubAddTwo,
    /// requires exactly one arg. Get the count from the $arg1.`try_into::<usize>`().
    TryInto,
}

/// Attributes for specifying how to compile a field
#[derive(Debug, Clone)]
pub(crate) enum CustomCompile {
    /// this field is ignored
    Skip,
    /// an inline is provided for calculating this field's value
    Expr(InlineExpr),
}

/// Attributes for specifying how to validate a field
#[derive(Debug, Clone)]
pub(crate) enum FieldValidation {
    /// this field is not validated
    Skip,
    /// the field is validated with a custom method.
    ///
    /// This must be a method with a &self param and a &mut ValidationCtx param.
    Custom(syn::Ident),
}

/// an inline expression used in an attribute
///
/// this has one fancy quality: you can reference fields of the current
/// object by prepending a '$' to the field name, e.g.
///
/// `#[count( $num_items - 1 )]`
#[derive(Debug, Clone)]
pub(crate) struct InlineExpr {
    pub(crate) expr: Box<syn::Expr>,
    // the expression used in a compilation context. This resolves any referenced
    // fields against `self`.
    compile_expr: Option<Box<syn::Expr>>,
    pub(crate) referenced_fields: Vec<syn::Ident>,
}

impl InlineExpr {
    pub(crate) fn compile_expr(&self) -> &syn::Expr {
        self.compile_expr.as_ref().unwrap_or(&self.expr)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FieldType {
    Offset {
        typ: syn::Ident,
        target: OffsetTarget,
    },
    Scalar {
        typ: syn::Ident,
    },
    Struct {
        typ: syn::Ident,
    },
    /// A type that may be a struct or a scalar.
    ///
    /// This only exists at parse time; when parsing is finished this will be
    /// resolved (or be an error).
    PendingResolution {
        typ: syn::Ident,
    },
    Array {
        inner_typ: Box<FieldType>,
    },
    ComputedArray(CustomArray),
    VarLenArray(CustomArray),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum OffsetTarget {
    Table(syn::Ident),
    Array(Box<FieldType>),
}

/// A representation shared between computed & varlen arrays
#[derive(Debug, Clone)]
pub(crate) struct CustomArray {
    span: Span,
    inner: syn::Ident,
    lifetime: Option<syn::Lifetime>,
}

impl PartialEq for CustomArray {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner && self.lifetime == other.lifetime
    }
}

impl Eq for CustomArray {}

impl Hash for CustomArray {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
        self.lifetime.hash(state);
    }
}

impl CustomArray {
    pub(crate) fn compile_type(&self) -> TokenStream {
        let inner = &self.inner;
        quote!(Vec<#inner>)
    }

    pub(crate) fn raw_inner_type(&self) -> &syn::Ident {
        &self.inner
    }

    pub(crate) fn type_with_lifetime(&self) -> TokenStream {
        let inner = &self.inner;
        if self.lifetime.is_some() {
            quote!(#inner<'a>)
        } else {
            inner.to_token_stream()
        }
    }

    pub(crate) fn span(&self) -> Span {
        self.span
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

#[derive(Debug, Default, Clone)]
pub(crate) struct EnumVariantAttrs {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) default: Option<syn::Path>,
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
    syn::custom_keyword!(skip);
    syn::custom_keyword!(scalar);
}

impl Parse for Items {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let mut items = IndexMap::new();
        let parse_module_path = get_parse_module_path(input)?;
        while !input.is_empty() {
            let item = input.parse::<Item>()?;
            items.insert(item.name().clone(), item);
        }
        Ok(Self {
            items,
            parse_module_path,
        })
    }
}

fn get_parse_module_path(input: ParseStream) -> syn::Result<syn::Path> {
    let attrs = input.call(Attribute::parse_inner)?;
    match attrs.as_slice() {
        [one] if one.path().is_ident("parse_module") => one.parse_args(),
        [one] => Err(logged_syn_error(one.span(), "unexpected attribute")),
        [_, two, ..] => Err(logged_syn_error(
            two.span(),
            "expected at most one top-level attribute",
        )),
        [] => Err(logged_syn_error(
            Span::call_site(),
            "expected #![parse_module(..)] attribute",
        )),
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
        content.parse::<Token![,]>()?;
        content.parse::<Token![$]>()?;
        let inner_field = content.parse()?;
        let content;
        let _ = braced!(content in input);
        let variants = Punctuated::<GroupVariant, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(GenericGroup {
            attrs,
            name,
            inner_type,
            inner_field,
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

impl Parse for Fields {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        let _ = braced!(content in input);
        let fields = Punctuated::<Field, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Self::new(fields)
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.parse()?;
        let name = input.parse::<syn::Ident>().unwrap();
        let _ = input.parse::<Token![:]>()?;
        let typ = input.parse()?;
        Ok(Field {
            attrs,
            name,
            typ,
            // computed later
            read_at_parse_time: false,
        })
    }
}

impl Parse for FieldType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let type_: syn::Type = input.parse()?;
        Self::from_syn_type(&type_)
    }
}

// https://learn.microsoft.com/en-us/typography/opentype/spec/otff#data-types
// Offset(16,24,32) get special handling, not listed here
// GlyphId, NameId, and MajorMinor are *not* spec names for scalar but are captured here
#[derive(Debug, PartialEq)]
enum WellKnownScalar {
    UInt8,
    Int8,
    UInt16,
    Int16,
    UInt24,
    Int24,
    UInt32,
    Int32,
    Fixed,
    FWord,
    UFWord,
    F2Dot14,
    LongDateTime,
    Tag,
    Version16Dot16,
    GlyphId16,
    NameId,
    MajorMinor,
}

impl FromStr for WellKnownScalar {
    type Err = ();

    // TODO(https://github.com/googlefonts/fontations/issues/84) use spec names
    fn from_str(str: &str) -> Result<WellKnownScalar, ()> {
        match str {
            "u8" => Ok(WellKnownScalar::UInt8),
            "i8" => Ok(WellKnownScalar::Int8),
            "u16" => Ok(WellKnownScalar::UInt16),
            "i16" => Ok(WellKnownScalar::Int16),
            "u24" => Ok(WellKnownScalar::UInt24),
            "Uint24" => Ok(WellKnownScalar::UInt24),
            "Int24" => Ok(WellKnownScalar::Int24),
            "i24" => Ok(WellKnownScalar::Int24),
            "u32" => Ok(WellKnownScalar::UInt32),
            "i32" => Ok(WellKnownScalar::Int32),
            "Fixed" => Ok(WellKnownScalar::Fixed),
            "FWord" => Ok(WellKnownScalar::FWord),
            "UfWord" => Ok(WellKnownScalar::UFWord),
            "F2Dot14" => Ok(WellKnownScalar::F2Dot14),
            "LongDateTime" => Ok(WellKnownScalar::LongDateTime),
            "Tag" => Ok(WellKnownScalar::Tag),
            "Version16Dot16" => Ok(WellKnownScalar::Version16Dot16),
            "GlyphId16" => Ok(WellKnownScalar::GlyphId16),
            "NameId" => Ok(WellKnownScalar::NameId),
            "MajorMinor" => Ok(WellKnownScalar::MajorMinor),
            _ => Err(()),
        }
    }
}

impl WellKnownScalar {
    fn from_path(path: &syn::PathSegment) -> Result<WellKnownScalar, ()> {
        if !path.arguments.is_empty() {
            return Err(());
        }
        WellKnownScalar::from_str(path.ident.to_string().as_str())
    }
}

impl FieldType {
    fn from_syn_type(type_: &syn::Type) -> syn::Result<Self> {
        // Figure out any "obvious" types, leave anything non-obvious for later

        if let syn::Type::Slice(slice) = type_ {
            let inner_type = FieldType::from_syn_type(&slice.elem)?;
            if matches!(inner_type, FieldType::Array { .. }) {
                return Err(logged_syn_error(
                    slice.elem.span(),
                    "nested arrays are invalid",
                ));
            }
            return Ok(FieldType::Array {
                inner_typ: Box::new(inner_type),
            });
        }

        let path = match type_ {
            syn::Type::Path(path) => &path.path,
            _ => return Err(logged_syn_error(type_.span(), "expected slice or path")),
        };

        let last = get_single_path_segment(path)?;

        if last.ident == "ComputedArray" || last.ident == "VarLenArray" {
            let inner_typ = get_single_generic_type_arg(&last.arguments)?;
            let inner = get_single_path_segment(&inner_typ)?;
            let lifetime = get_single_lifetime(&inner.arguments)?;
            let array = CustomArray {
                span: inner.span(),
                inner: inner.ident.clone(),
                lifetime,
            };
            if last.ident == "ComputedArray" {
                return Ok(FieldType::ComputedArray(array));
            } else {
                return Ok(FieldType::VarLenArray(array));
            }
        }

        if WellKnownScalar::from_path(last).is_ok() {
            return Ok(FieldType::Scalar {
                typ: last.ident.clone(),
            });
        }

        if ["Offset16", "Offset24", "Offset32"].contains(&last.ident.to_string().as_str()) {
            let target = get_offset_target(last)?;
            return Ok(FieldType::Offset {
                typ: last.ident.clone(),
                target,
            });
        }

        // We'll figure it out later, what could go wrong?
        if !last.arguments.is_empty() {
            return Err(logged_syn_error(path.span(), "Unexpected path arguments"));
        }
        debug!("Pending {}", quote! { #last });
        Ok(FieldType::PendingResolution {
            typ: last.ident.clone(),
        })
    }
}

fn get_single_path_segment(path: &syn::Path) -> syn::Result<&syn::PathSegment> {
    if path.segments.len() != 1 {
        return Err(logged_syn_error(path.span(), "expect a single-item path"));
    }
    Ok(path.segments.last().unwrap())
}

// either a single ident or an array
fn get_offset_target(input: &syn::PathSegment) -> syn::Result<OffsetTarget> {
    match get_single_generic_arg(&input.arguments)? {
        Some(syn::GenericArgument::Type(syn::Type::Slice(t))) => {
            let inner = FieldType::from_syn_type(&t.elem)?;
            if matches!(
                inner,
                FieldType::Scalar { .. }
                    | FieldType::Struct { .. }
                    | FieldType::PendingResolution { .. }
            ) {
                Ok(OffsetTarget::Array(Box::new(inner)))
            } else {
                Err(logged_syn_error(
                    t.elem.span(),
                    "offsets can only point to arrays of records or scalars",
                ))
            }
        }
        Some(syn::GenericArgument::Type(syn::Type::Path(t)))
            if t.path.segments.len() == 1 && t.path.get_ident().is_some() =>
        {
            Ok(OffsetTarget::Table(t.path.get_ident().unwrap().clone()))
        }
        Some(_) => Err(logged_syn_error(input.span(), "expected path or slice")),
        None => Err(logged_syn_error(input.span(), "expected offset target")),
    }
}

impl Parse for FieldReadArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut inputs = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![$]>()?;
            inputs.push(input.parse::<syn::Ident>()?);
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(FieldReadArgs { inputs })
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

static MATCH_IF: &str = "match_if";
static WRITE_FONTS_ONLY: &str = "write_fonts_only";

impl Parse for VariantAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut this = VariantAttrs::default();
        let attrs = Attribute::parse_outer(input)
            .map_err(|e| syn::Error::new(e.span(), format!("hmm: '{e}'")))?;

        for attr in attrs {
            let ident = attr.path().get_ident().ok_or_else(|| {
                syn::Error::new(
                    attr.path().span(),
                    "attr paths should be a single identifier",
                )
            })?;
            if ident == DOC {
                this.docs.push(attr);
            } else if ident == MATCH_IF {
                this.match_stmt = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == WRITE_FONTS_ONLY {
                this.write_only = Some(attr.path().clone());
            } else {
                return Err(logged_syn_error(
                    ident.span(),
                    format!("unknown variant attribute {ident}"),
                ));
            }
        }
        Ok(this)
    }
}

impl FieldAttrs {
    // returns an error if multiple condition attributes are present, which I hope
    // to not need to support
    fn checked_set_condition(
        &mut self,
        ident: &syn::Ident,
        condition: Condition,
    ) -> syn::Result<()> {
        if let Some(existing) = &self.conditional {
            return Err(syn::Error::new(
                ident.span(),
                format!(
                    "condition conflicts with existing condition {}",
                    existing.name
                ),
            ));
        }
        self.conditional = Some(Attr::new(ident.clone(), condition));
        Ok(())
    }
}

static DOC: &str = "doc";
static NULLABLE: &str = "nullable";
static SKIP_GETTER: &str = "skip_getter";
static COUNT: &str = "count";
static SINCE_VERSION: &str = "since_version";
static IF_COND: &str = "if_cond";
static IF_FLAG: &str = "if_flag";
static FORMAT: &str = "format";
static VERSION: &str = "version";
static OFFSET_GETTER: &str = "offset_getter";
static OFFSET_DATA: &str = "offset_data_method";
static OFFSET_ADJUSTMENT: &str = "offset_adjustment";
static COMPILE: &str = "compile";
static COMPILE_WITH: &str = "compile_with";
static COMPILE_TYPE: &str = "compile_type";
static DEFAULT: &str = "default";
static READ_WITH: &str = "read_with";
static READ_OFFSET_WITH: &str = "read_offset_with";
static TRAVERSE_WITH: &str = "traverse_with";
static TO_OWNED: &str = "to_owned";
static VALIDATE: &str = "validate";

impl Parse for FieldAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut this = FieldAttrs::default();
        let attrs = Attribute::parse_outer(input)
            .map_err(|e| syn::Error::new(e.span(), format!("hmm: '{e}'")))?;

        for attr in attrs {
            let ident = attr.path().get_ident().ok_or_else(|| {
                syn::Error::new(
                    attr.path().span(),
                    "attr paths should be a single identifier",
                )
            })?;
            if ident == DOC {
                this.docs.push(attr);
            } else if ident == NULLABLE {
                this.nullable = Some(attr.path().clone());
            } else if ident == SKIP_GETTER {
                this.skip_getter = Some(attr.path().clone());
            } else if ident == OFFSET_GETTER {
                this.offset_getter = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == OFFSET_DATA {
                this.offset_data = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == OFFSET_ADJUSTMENT {
                this.offset_adjustment = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == VERSION {
                this.version = Some(attr.path().clone());
            } else if ident == COUNT {
                this.count = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == COMPILE {
                this.compile = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == COMPILE_WITH {
                this.compile_with = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == COMPILE_TYPE {
                this.compile_type = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == DEFAULT {
                this.default = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == VALIDATE {
                this.validate = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == TO_OWNED {
                this.to_owned = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == SINCE_VERSION {
                let spec = attr.parse_args()?;
                this.checked_set_condition(ident, Condition::SinceVersion(spec))?;
            } else if ident == IF_FLAG {
                let condition = parse_if_flag(&attr)?;
                this.checked_set_condition(ident, condition)?;
            } else if ident == IF_COND {
                let condition = parse_if_cond(&attr)?;
                this.checked_set_condition(ident, condition)?;
            } else if ident == READ_WITH {
                this.read_with_args = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == READ_OFFSET_WITH {
                this.read_offset_args = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == TRAVERSE_WITH {
                this.traverse_with = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == FORMAT {
                this.format = Some(Attr::new(ident.clone(), parse_attr_eq_value(&attr)?))
            } else {
                return Err(logged_syn_error(
                    ident.span(),
                    format!("unknown field attribute {ident}"),
                ));
            }
        }
        Ok(this)
    }
}

static SKIP_FROM_OBJ: &str = "skip_from_obj";
static SKIP_FONT_WRITE: &str = "skip_font_write";
static SKIP_CONSTRUCTOR: &str = "skip_constructor";
static READ_ARGS: &str = "read_args";
static GENERIC_OFFSET: &str = "generic_offset";
static TAG: &str = "tag";

impl Parse for TableAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut this = TableAttrs::default();
        let attrs = Attribute::parse_outer(input)
            .map_err(|e| syn::Error::new(e.span(), format!("hmm: '{e}'")))?;

        for attr in attrs {
            let ident = attr.path().get_ident().ok_or_else(|| {
                syn::Error::new(
                    attr.path().span(),
                    "attr paths should be a single identifier",
                )
            })?;
            if ident == DOC {
                this.docs.push(attr);
            } else if ident == SKIP_FROM_OBJ {
                this.skip_from_obj = Some(attr.path().clone());
            } else if ident == SKIP_FONT_WRITE {
                this.skip_font_write = Some(attr.path().clone());
            } else if ident == SKIP_CONSTRUCTOR {
                this.skip_constructor = Some(attr.path().clone());
            } else if ident == WRITE_FONTS_ONLY {
                this.write_only = Some(attr.path().clone());
            } else if ident == READ_ARGS {
                this.read_args = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == GENERIC_OFFSET {
                this.generic_offset = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == TAG {
                let tag: syn::LitStr = parse_attr_eq_value(&attr)?;
                if let Err(e) = Tag::new_checked(tag.value().as_bytes()) {
                    return Err(logged_syn_error(tag.span(), format!("invalid tag: '{e}'")));
                }
                this.tag = Some(Attr::new(ident.clone(), tag))
            } else if ident == VALIDATE {
                this.validate = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else {
                return Err(logged_syn_error(
                    ident.span(),
                    format!("unknown table attribute {ident}"),
                ));
            }
        }
        Ok(this)
    }
}

impl Parse for EnumVariantAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut this = EnumVariantAttrs::default();
        let attrs = Attribute::parse_outer(input)
            .map_err(|e| syn::Error::new(e.span(), format!("hmm: '{e}'")))?;

        for attr in attrs {
            let ident = attr.path().get_ident().ok_or_else(|| {
                syn::Error::new(
                    attr.path().span(),
                    "attr paths should be a single identifier",
                )
            })?;
            if ident == DOC {
                this.docs.push(attr);
            } else if ident == DEFAULT {
                this.default = Some(attr.path().clone());
            } else {
                return Err(logged_syn_error(
                    ident.span(),
                    format!("unknown field attribute {ident}"),
                ));
            }
        }
        Ok(this)
    }
}

impl Parse for TableReadArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let args = Punctuated::<TableReadArg, Token![,]>::parse_separated_nonempty(input)?
            .into_iter()
            .collect();
        Ok(TableReadArgs { args })
    }
}

impl Parse for TableReadArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse::<syn::Ident>()?;
        input.parse::<Token![:]>()?;
        let typ = input.parse()?;
        Ok(TableReadArg { ident, typ })
    }
}

fn resolve_ident<'a>(
    known: &'a HashMap<syn::Ident, FieldType>,
    field_name: &syn::Ident,
    field_type: &syn::Ident,
) -> Result<&'a FieldType, syn::Error> {
    if let Some(item) = known.get(field_type) {
        debug!("Resolve {}: {} to {:?}", field_name, field_type, item);
        Ok(item)
    } else {
        Err(logged_syn_error(
            field_type.span(),
            "Error: undeclared type. Missing a record, table, extern table, or extern record?",
        ))
    }
}

fn resolve_field(
    known: &HashMap<syn::Ident, FieldType>,
    field: &mut Field,
) -> Result<(), syn::Error> {
    if let FieldType::PendingResolution { typ } = &field.typ {
        let resolved_typ = resolve_ident(known, &field.name, typ)?;
        *field = Field {
            typ: resolved_typ.clone(),
            ..field.clone()
        }
    }

    // Array and offsets can nest FieldType, pursue the rabbit
    if let FieldType::Array { inner_typ } = &field.typ {
        if let FieldType::PendingResolution { typ } = inner_typ.as_ref() {
            let resolved_typ = resolve_ident(known, &field.name, typ)?;
            *field = Field {
                typ: FieldType::Array {
                    inner_typ: Box::new(resolved_typ.clone()),
                },
                ..field.clone()
            }
        }
    }

    if let FieldType::Offset { typ, target } = &field.typ {
        let offset_typ = typ;
        if let OffsetTarget::Array(array_of) = target {
            if let FieldType::PendingResolution { typ } = array_of.as_ref() {
                let resolved_typ = resolve_ident(known, &field.name, typ)?;
                *field = Field {
                    typ: FieldType::Offset {
                        typ: offset_typ.clone(),
                        target: OffsetTarget::Array(Box::new(resolved_typ.clone())),
                    },
                    ..field.clone()
                }
            }
        }
    }
    Ok(())
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
            for field in fields {
                resolve_field(&known, field)?;
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

impl RawEnum {
    pub(crate) fn sanity_check(&self, _: Phase) -> syn::Result<()> {
        let defaults: Vec<_> = self
            .variants
            .iter()
            .filter_map(|v| v.attrs.default.as_ref().map(|_| &v.name))
            .collect();
        if defaults.len() > 1 {
            return Err(logged_syn_error(
                self.name.span(),
                format!("multiple defaults {defaults:?}"),
            ));
        }
        Ok(())
    }
}

impl Parse for CountArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![$]) {
            input.parse::<Token![$]>()?;
            input.parse().map(Self::Field)
        } else {
            let int = input.parse::<syn::LitInt>()?;
            let digits = int.base10_digits();
            if digits.starts_with('-') {
                return Err(syn::Error::new(
                    input.span(),
                    "negative count is not supported",
                ));
            }
            //HACK: we ensure these literals always have an explicit type.
            //Is this necessary? no clue.
            if int.suffix() != "usize" {
                let reparse = format!("{digits}_usize");
                syn::parse_str(&reparse).map(Self::Literal)
            } else {
                Ok(Self::Literal(int))
            }
        }
    }
}

impl Parse for Count {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![..]) {
            input.parse().map(Count::All)
        } else if input.peek(syn::Ident) {
            // leading ident must be a function
            let xform = input.parse()?;
            let content;
            let _ = parenthesized!(content in input);
            let args = Punctuated::<CountArg, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            Count::try_from_fancy_stuff(input.span(), xform, args)
        } else {
            input.parse().map(Self::SingleArg)
        }
    }
}

impl Parse for CountTransform {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse::<syn::Ident>()?;
        CountTransform::from_str(&ident.to_string())
            .map_err(|err| syn::Error::new(ident.span(), err))
    }
}

static TRANSFORM_IDENTS: &[(CountTransform, &str)] = &[
    (CountTransform::Sub, "subtract"),
    (CountTransform::Add, "add"),
    (CountTransform::AddMul, "add_multiply"),
    (CountTransform::MulAdd, "multiply_add"),
    (CountTransform::Half, "half"),
    (CountTransform::DeltaValueCount, "delta_value_count"),
    (CountTransform::DeltaSetIndexData, "delta_set_index_data"),
    (CountTransform::TupleLen, "tuple_len"),
    (
        CountTransform::ItemVariationDataLen,
        "item_variation_data_len",
    ),
    (CountTransform::BitmapLen, "bitmap_len"),
    (CountTransform::MaxValueBitmapLen, "max_value_bitmap_len"),
    (CountTransform::SubAddTwo, "subtract_add_two"),
    (CountTransform::TryInto, "try_into"),
];

impl FromStr for CountTransform {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TRANSFORM_IDENTS
            .iter()
            .find_map(|(var, ident)| (*ident == s).then_some(*var))
            .ok_or_else(|| {
                format!(
                    "invalid transform, expected one of {}",
                    TRANSFORM_IDENTS
                        .iter()
                        .map(|(_, ident)| format!("'{ident}'"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }
}

impl CountTransform {
    fn arg_count(self) -> usize {
        match self {
            CountTransform::Sub => 2,
            CountTransform::Add => 2,
            CountTransform::AddMul => 3,
            CountTransform::MulAdd => 3,
            CountTransform::Half => 1,
            CountTransform::DeltaValueCount => 3,
            CountTransform::DeltaSetIndexData => 2,
            CountTransform::TupleLen => 3,
            CountTransform::ItemVariationDataLen => 3,
            CountTransform::BitmapLen => 1,
            CountTransform::MaxValueBitmapLen => 1,
            CountTransform::SubAddTwo => 2,
            CountTransform::TryInto => 1,
        }
    }
}

impl ToTokens for CountArg {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            CountArg::Field(fld) => fld.to_tokens(tokens),
            CountArg::Literal(lit) => lit.to_tokens(tokens),
        }
    }
}

impl Parse for VersionSpec {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fork = input.fork();
        if fork.parse::<syn::LitInt>().is_ok() && fork.is_empty() {
            let major = input.parse::<syn::LitInt>()?;
            let major: u16 = major.base10_parse()?;
            return Ok(VersionSpec { major, minor: None });
        }

        let version = input.parse::<syn::LitFloat>()?;
        let Some((major, minor)) = version.base10_digits().split_once('.') else {
            return Err(syn::Error::new(version.span(), "version should be single integer major or major.minor (e.g. '1', '4', '1.1', '2.5')"));
        };
        let major = major.parse::<u16>();
        let minor = minor.parse::<u16>();

        major
            .and_then(|major| {
                minor.map(|minor| VersionSpec {
                    major,
                    minor: Some(minor),
                })
            })
            .map_err(|_| syn::Error::new(version.span(), "could not parse major/minor version"))
    }
}

impl ToTokens for VersionSpec {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let major = &self.major;
        if let Some(minor) = &self.minor {
            tokens.extend(quote!( (#major, #minor) ));
        } else {
            major.to_tokens(tokens);
        }
    }
}

impl Parse for CustomCompile {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fork = input.fork();
        if fork.parse::<kw::skip>().is_ok() && fork.is_empty() {
            input.parse::<kw::skip>()?;
            return Ok(Self::Skip);
        }

        input.parse().map(Self::Expr)
    }
}

impl Parse for FieldValidation {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fork = input.fork();
        if fork.parse::<kw::skip>().is_ok() && fork.is_empty() {
            input.parse::<kw::skip>()?;
            return Ok(Self::Skip);
        }

        input.parse().map(Self::Custom)
    }
}

impl Count {
    fn try_from_fancy_stuff(
        err_span: Span,
        xform: CountTransform,
        args: Vec<CountArg>,
    ) -> Result<Self, syn::Error> {
        let expected_arg_count = xform.arg_count();
        if args.len() != expected_arg_count {
            return Err(syn::Error::new(
                err_span,
                format!("expected {expected_arg_count} arguments"),
            ));
        }
        Ok(Count::Complicated { args, xform })
    }

    pub(crate) fn single_field(&self) -> Option<&syn::Ident> {
        if let Count::SingleArg(CountArg::Field(ident)) = self {
            Some(ident)
        } else {
            None
        }
    }

    pub(crate) fn iter_referenced_fields(&self) -> impl Iterator<Item = &syn::Ident> {
        let (one, two) = match self {
            Self::SingleArg(CountArg::Field(ident)) => (Some(ident), None),
            Self::Complicated { args, .. } => (
                None,
                Some(args.iter().filter_map(|arg| match arg {
                    CountArg::Field(ident) => Some(ident),
                    _ => None,
                })),
            ),
            _ => (None, None),
        };
        // a trick so we return the exact sample iterator type from both match arms
        one.into_iter().chain(two.into_iter().flatten())
    }

    pub(crate) fn count_expr(&self) -> TokenStream {
        match self {
            Count::All(_) => unreachable!("'all' count handled separately"),
            Count::SingleArg(CountArg::Field(arg)) => quote!(#arg as usize),
            Count::SingleArg(CountArg::Literal(arg)) => quote!(#arg),
            Count::Complicated { args, xform } => match (xform, args.as_slice()) {
                (CountTransform::Sub, [a, b]) => {
                    quote!(transforms::subtract(#a, #b))
                }
                (CountTransform::Add, [a, b]) => {
                    quote!(transforms::add(#a, #b))
                }
                (CountTransform::AddMul, [a, b, c]) => {
                    quote!(transforms::add_multiply(#a, #b, #c))
                }
                (CountTransform::MulAdd, [a, b, c]) => {
                    quote!(transforms::multiply_add(#a, #b, #c))
                }
                (CountTransform::Half, [a]) => {
                    quote!(transforms::half(#a))
                }
                (CountTransform::DeltaSetIndexData, [a, b]) => {
                    quote!(EntryFormat::map_size(#a, #b))
                }
                (CountTransform::DeltaValueCount, [a, b, c]) => {
                    quote!(DeltaFormat::value_count(#a, #b, #c))
                }
                (CountTransform::TupleLen, [a, b, c]) => {
                    quote!(TupleIndex::tuple_len(#a, #b, #c))
                }
                (CountTransform::ItemVariationDataLen, [a, b, c]) => {
                    quote!(ItemVariationData::delta_sets_len(#a, #b, #c))
                }
                (CountTransform::BitmapLen, [a]) => {
                    quote!(transforms::bitmap_len(#a))
                }
                (CountTransform::MaxValueBitmapLen, [a]) => {
                    quote!(transforms::max_value_bitmap_len(#a))
                }
                (CountTransform::SubAddTwo, [a, b]) => {
                    quote!(transforms::subtract_add_two(#a, #b))
                }
                (CountTransform::TryInto, [a]) => {
                    quote!(usize::try_from(#a).unwrap_or_default())
                }
                _ => unreachable!("validated before now"),
            },
        }
    }
}

impl Parse for InlineExpr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        fn parse_inline_expr(tokens: TokenStream) -> syn::Result<InlineExpr> {
            let span = tokens.span();
            let s = tokens.to_string();
            let mut idents = Vec::new();
            let find_dollar_idents = regex::Regex::new(r"(\$) (\w+)").unwrap();
            for ident in find_dollar_idents.captures_iter(&s) {
                let text = ident.get(2).unwrap().as_str();
                let ident = syn::parse_str::<syn::Ident>(text).map_err(|_| {
                    syn::Error::new(tokens.span(), format!("invalid ident '{text}'"))
                })?;
                idents.push(ident);
            }
            let expr: syn::Expr = if idents.is_empty() {
                syn::parse2(tokens)
            } else {
                let new_source = find_dollar_idents.replace_all(&s, "$2");
                syn::parse_str(&new_source)
            }
            .map_err(|_| syn::Error::new(span, "failed to parse expression"))?;

            let compile_expr = (!idents.is_empty())
                .then(|| {
                    let new_source =
                        find_dollar_idents.replace_all(&s, replace_field_with_compile_field);
                    syn::parse_str::<syn::Expr>(&new_source)
                })
                .transpose()?
                .map(Box::new);

            idents.sort_unstable();
            idents.dedup();

            Ok(InlineExpr {
                expr: expr.into(),
                compile_expr,
                referenced_fields: idents,
            })
        }

        let tokens: TokenStream = input.parse()?;
        parse_inline_expr(tokens)
    }
}

fn replace_field_with_compile_field(captures: &Captures) -> String {
    let ident = captures.get(2).unwrap().as_str();
    let ident = crate::fields::remove_offset_from_field_name(ident);
    format!("&self.{ident}")
}

impl NeededWhen {
    fn at_parsetime(&self) -> bool {
        matches!(self, NeededWhen::Parse | NeededWhen::Both)
    }

    fn at_runtime(&self) -> bool {
        matches!(self, NeededWhen::Runtime | NeededWhen::Both)
    }
}

impl ReferencedFields {
    pub(crate) fn needs_at_parsetime(&self, ident: &syn::Ident) -> bool {
        self.0
            .get(ident)
            .map(NeededWhen::at_parsetime)
            .unwrap_or(false)
    }

    pub(crate) fn needs_at_runtime(&self, ident: &syn::Ident) -> bool {
        self.0
            .get(ident)
            .map(NeededWhen::at_runtime)
            .unwrap_or(false)
    }
}

impl OffsetTarget {
    pub(crate) fn getter_return_type(&self, is_generic: bool) -> TokenStream {
        match self {
            OffsetTarget::Table(ident) if !is_generic => quote!(Result<#ident <'a>, ReadError>),
            OffsetTarget::Table(ident) => quote!(Result<#ident, ReadError>),
            OffsetTarget::Array(inner) => {
                let elem_type = match inner.deref() {
                    FieldType::Scalar { typ } => quote!(BigEndian<#typ>),
                    FieldType::Struct { typ } => typ.to_token_stream(),
                    _ => panic!("we should have returned a humane error before now"),
                };
                quote!(Result<&'a [#elem_type], ReadError>)
            }
        }
    }

    pub(crate) fn compile_type(&self) -> TokenStream {
        match self {
            Self::Table(ident) => ident.to_token_stream(),
            Self::Array(thing) => {
                let cooked = thing.cooked_type_tokens();
                quote!(Vec<#cooked>)
            }
        }
    }
}

impl FromIterator<(syn::Ident, NeededWhen)> for ReferencedFields {
    fn from_iter<T: IntoIterator<Item = (syn::Ident, NeededWhen)>>(iter: T) -> Self {
        let mut map = HashMap::new();
        for (key, new_val) in iter {
            let value = map.entry(key).or_insert(new_val);
            // if a value is referenced by multiple fields, we combine them
            *value = match (*value, new_val) {
                (NeededWhen::Parse, NeededWhen::Parse) => NeededWhen::Parse,
                (NeededWhen::Runtime, NeededWhen::Runtime) => NeededWhen::Runtime,
                _ => NeededWhen::Both,
            };
        }
        Self(map)
    }
}

fn parse_attr_eq_value<T: Parse>(attr: &syn::Attribute) -> syn::Result<T> {
    /// the tokens '= T' where 'T' is any `Parse`
    struct EqualsThing<T>(T);

    impl<T: Parse> Parse for EqualsThing<T> {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            input.parse::<Token![=]>()?;
            input.parse().map(EqualsThing)
        }
    }
    let tokens = attr.meta.require_name_value()?.value.to_token_stream();
    syn::parse2::<T>(tokens).map_err(|err| syn::Error::new(attr.meta.span(), err.to_string()))
}

fn parse_if_flag(attr: &syn::Attribute) -> syn::Result<Condition> {
    struct IfFlag(syn::Ident, syn::Path);
    impl Parse for IfFlag {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            input.parse::<Token![$]>()?;
            let ident = input.parse::<syn::Ident>()?;
            input.parse::<Token![,]>()?;
            let path = input.parse::<syn::Path>()?;
            Ok(IfFlag(ident, path))
        }
    }

    attr.parse_args::<IfFlag>()
        .map(|IfFlag(field, flag)| Condition::IfFlag { field, flag })
        .map_err(|e| {
            syn::Error::new(
                e.span(),
                format!("expected #[if_flag($field_name, FlagType::SOME_FLAG)]: '{e}'"),
            )
        })
}

impl Parse for IfArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![$]) {
            input.parse::<Token![$]>()?;
            input.parse().map(Self::Field)
        } else {
            input.parse().map(Self::Path)
        }
    }
}

impl Parse for IfTransform {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse::<syn::Ident>()?;
        let content;
        let _ = parenthesized!(content in input);
        let args: Vec<IfArg> = Punctuated::<IfArg, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();

        IfTransform::from_args(&ident.to_string(), args)
            .map_err(|err| syn::Error::new(ident.span(), err))
    }
}

impl IfTransform {
    fn from_args(s: &str, args: Vec<IfArg>) -> Result<Self, String> {
        match s {
            "any_flag" => Self::any_flag(args),
            _ => Err(format!("invalid if_cond transform function: {}", s)),
        }
    }

    fn any_flag(args: Vec<IfArg>) -> Result<Self, String> {
        let Some(IfArg::Field(field)) = args.first() else {
            return Err("First argument to any_flag must be a field name.".to_string());
        };

        let mut flags: Vec<syn::Path> = vec![];
        for arg in args.iter().skip(1) {
            let IfArg::Path(flag) = arg else {
                return Err(
                    "Arguments after the first argument to any_flag must be a flag names."
                        .to_string(),
                );
            };
            flags.push(flag.clone());
        }

        Ok(IfTransform::AnyFlag(field.clone(), flags))
    }

    pub(crate) fn input_field(&self) -> Vec<syn::Ident> {
        match self {
            IfTransform::AnyFlag(field, _) => vec![field.clone()],
        }
    }
}

fn parse_if_cond(attr: &syn::Attribute) -> syn::Result<Condition> {
    struct If(IfTransform);
    impl Parse for If {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            // leading ident must be a function
            if !input.peek(syn::Ident) {
                return Err(syn::Error::new(
                    input.span(),
                    "Non identifier argument to if_cond().",
                ));
            }

            let xform: IfTransform = input.parse()?;
            Ok(If(xform))
        }
    }

    attr.parse_args::<If>()
        .map(|If(xform)| Condition::IfCond { xform })
        .map_err(|e| {
            syn::Error::new(
                e.span(),
                format!("expected #[if_cond(condition_function(...))]: '{e}'"),
            )
        })
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

fn get_single_generic_type_arg(input: &syn::PathArguments) -> syn::Result<syn::Path> {
    match get_single_generic_arg(input)? {
        Some(syn::GenericArgument::Type(syn::Type::Path(path)))
            if path.qself.is_none() && path.path.segments.len() == 1 =>
        {
            Ok(path.path.clone())
        }
        _ => Err(logged_syn_error(input.span(), "expected type")),
    }
}

fn get_single_generic_arg(
    input: &syn::PathArguments,
) -> syn::Result<Option<&syn::GenericArgument>> {
    match input {
        syn::PathArguments::None => Ok(None),
        syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
            Ok(Some(args.args.last().unwrap()))
        }
        _ => Err(logged_syn_error(
            input.span(),
            "expected single generic argument",
        )),
    }
}

fn get_single_lifetime(input: &syn::PathArguments) -> syn::Result<Option<syn::Lifetime>> {
    match get_single_generic_arg(input)? {
        None => Ok(None),
        Some(syn::GenericArgument::Lifetime(arg)) => Ok(Some(arg.clone())),
        _ => Err(logged_syn_error(input.span(), "expected single lifetime")),
    }
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

    fn make_path_seg(s: &str) -> syn::PathSegment {
        let path = syn::parse_str::<syn::Path>(s).unwrap();
        path.segments.last().unwrap().clone()
    }

    fn parse_count(s: &str) -> Result<Count, syn::Error> {
        syn::parse_str(s)
    }

    #[test]
    fn offset_target() {
        let array_target = make_path_seg("Offset16<[u16]>");
        assert!(get_offset_target(&array_target).is_ok());

        let path_target = make_path_seg("Offset16<SomeType>");
        assert!(get_offset_target(&path_target).is_ok());

        let non_target = make_path_seg("Offset16");
        assert!(get_offset_target(&non_target).is_err());

        let tuple_target = make_path_seg("Offset16<(u16, u16)>");
        assert!(get_offset_target(&tuple_target).is_err());
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
