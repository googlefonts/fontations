//! Attribute parsing for tables, fields, variants, and enums.

use std::str::FromStr;

use font_types::Tag;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use regex::Captures;
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Token,
};

use super::logged_syn_error;

mod kw {
    syn::custom_keyword!(skip);
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
    /// Marks this field as the discriminant for a generic offset type.
    pub(crate) discriminant: Option<syn::Path>,
    /// During sanitize, only check the length of this field (don't recurse).
    pub(crate) sanitize_len_only: Option<syn::Path>,
    /// Custom sanitize fn, like compile_with but for sanitize.
    pub(crate) sanitize_with: Option<Attr<SanitizeWith>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VariantAttrs {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) match_stmt: Option<Attr<InlineExpr>>,
    pub(crate) write_only: Option<syn::Path>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct EnumVariantAttrs {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) default: Option<syn::Path>,
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
pub(crate) struct FieldReadArgs {
    pub(crate) inputs: Vec<syn::Ident>,
}

#[derive(Debug, Clone)]
pub(crate) struct SanitizeWith {
    pub(crate) fn_name: syn::Ident,
    pub(crate) inputs: Vec<syn::Ident>,
}

#[derive(Clone, Debug)]
pub(crate) enum Condition {
    SinceVersion(VersionSpec),
    IfFlag { field: syn::Ident, flag: syn::Path },
}

#[derive(Clone, Debug)]
pub(crate) struct VersionSpec {
    pub(crate) major: u16,
    pub(crate) minor: Option<u16>,
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

// ── FieldAttrs helpers ──────────────────────────────────────────────

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

// ── Attribute name constants ────────────────────────────────────────

static DOC: &str = "doc";
static NULLABLE: &str = "nullable";
static SKIP_GETTER: &str = "skip_getter";
static COUNT: &str = "count";
static SINCE_VERSION: &str = "since_version";
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
static DISCRIMINANT: &str = "discriminant";
static SANITIZE_LEN_ONLY: &str = "sanitize_len_only";
static SANITIZE_WITH: &str = "sanitize_with";

static MATCH_IF: &str = "match_if";
static WRITE_FONTS_ONLY: &str = "write_fonts_only";

static SKIP_FROM_OBJ: &str = "skip_from_obj";
static SKIP_FONT_WRITE: &str = "skip_font_write";
static SKIP_CONSTRUCTOR: &str = "skip_constructor";
static READ_ARGS: &str = "read_args";
static GENERIC_OFFSET: &str = "generic_offset";
static TAG: &str = "tag";

// ── Parse impls for attr containers ─────────────────────────────────

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
            } else if ident == READ_WITH {
                this.read_with_args = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == READ_OFFSET_WITH {
                this.read_offset_args = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == TRAVERSE_WITH {
                this.traverse_with = Some(Attr::new(ident.clone(), attr.parse_args()?));
            } else if ident == FORMAT {
                this.format = Some(Attr::new(ident.clone(), parse_attr_eq_value(&attr)?))
            } else if ident == DISCRIMINANT {
                this.discriminant = Some(attr.path().clone());
            } else if ident == SANITIZE_LEN_ONLY {
                this.sanitize_len_only = Some(attr.path().clone());
            } else if ident == SANITIZE_WITH {
                this.sanitize_with = Some(Attr::new(ident.clone(), attr.parse_args()?));
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

// ── Parse impls for attr value types ────────────────────────────────

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

impl Parse for SanitizeWith {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fn_name = input.parse::<syn::Ident>()?;
        let mut inputs = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            input.parse::<Token![$]>()?;
            inputs.push(input.parse::<syn::Ident>()?);
        }
        Ok(SanitizeWith { fn_name, inputs })
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

    pub(crate) fn lit_int(&self) -> Option<&syn::LitInt> {
        if let Count::SingleArg(CountArg::Literal(int)) = self {
            Some(int)
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

// ── Helper functions ────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_field_attrs(input: &str) -> syn::Result<FieldAttrs> {
        syn::parse_str(input)
    }

    #[test]
    fn parse_sanitize_with() {
        let attrs = parse_field_attrs("#[sanitize_with(my_custom_sanitize)]").unwrap();
        let sanitize = attrs.sanitize_with.expect("should have sanitize_with");
        assert_eq!(sanitize.attr.fn_name.to_string(), "my_custom_sanitize");
        assert!(sanitize.attr.inputs.is_empty());
    }

    #[test]
    fn parse_sanitize_with_args() {
        let attrs =
            parse_field_attrs("#[sanitize_with(my_custom_sanitize, $value_format)]").unwrap();
        let sanitize = attrs.sanitize_with.expect("should have sanitize_with");
        assert_eq!(sanitize.attr.fn_name.to_string(), "my_custom_sanitize");
        assert_eq!(sanitize.attr.inputs.len(), 1);
        assert_eq!(sanitize.attr.inputs[0].to_string(), "value_format");
    }

    #[test]
    fn sanitize_with_requires_arg() {
        assert!(parse_field_attrs("#[sanitize_with]").is_err());
    }
}
