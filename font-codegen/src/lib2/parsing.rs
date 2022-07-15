//! raw parsing code

use std::collections::HashSet;

use proc_macro2::{TokenStream, TokenTree};
use quote::{quote, ToTokens};
use syn::{
    braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token, Attribute, Token,
};

pub(crate) struct Items {
    //pub(crate) use_stmts: Vec<SimpleUse>,
    pub(crate) items: Vec<Item>,
}

pub(crate) enum Item {
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
    pub(crate) fields: Fields,
}

#[derive(Debug, Clone)]
pub(crate) struct Record {
    //pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub(crate) fields: Fields,
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
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) name: syn::Ident,
    pub(crate) typ: syn::Ident,
}

#[derive(Debug, Clone)]
pub(crate) struct Fields {
    fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub(crate) struct Field {
    //pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) attrs: FieldAttrs,
    pub(crate) name: syn::Ident,
    pub(crate) typ: FieldType,
    /// `true` if this field is required to be read in order to parse subsequent
    /// fields.
    ///
    /// For instance: in a versioned table, the version must be read to determine
    /// whether to expect version-dependent fields.
    read_at_parse_time: bool,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct FieldAttrs {
    pub(crate) docs: Vec<syn::Attribute>,
    pub(crate) nullable: Option<syn::Path>,
    pub(crate) available: Option<syn::Path>,
    pub(crate) no_getter: Option<syn::Path>,
    pub(crate) version: Option<syn::Path>,
    pub(crate) format: Option<FormatAttr>,
    pub(crate) count: Option<InlineExpr>,
    pub(crate) len: Option<InlineExpr>,
}

#[derive(Debug, Clone)]
pub(crate) struct FormatAttr {
    kw: syn::Ident,
    pub(crate) value: syn::LitInt,
}

///// Annotations for how to calculate the count of an array.
//#[derive(Debug, Clone)]
//pub(crate) enum Count {
////Field(syn::Ident),
//Literal(syn::LitInt),
//All(syn::Path),
//Expr(InlineExpr),
////Function {
////fn_: syn::Path,
////args: Vec<syn::Ident>,
////},
//}

/// an inline expression used in an attribute
///
/// this has one fancy quality: you can reference fields of the current
/// object by prepending a '$' to the field name, e.g.
///
/// `#[count( $num_items - 1 )]`
#[derive(Debug, Clone)]
pub(crate) struct InlineExpr {
    pub(crate) expr: syn::Expr,
    pub(crate) referenced_fields: Vec<syn::Ident>,
}

#[derive(Debug, Clone)]
pub(crate) enum FieldType {
    Offset { typ: syn::Ident },
    Scalar { typ: syn::Ident },
    Other { typ: syn::Ident },
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

impl Fields {
    fn new(mut fields: Vec<Field>) -> syn::Result<Self> {
        let referenced_fields = fields
            .iter()
            .flat_map(Field::input_fields)
            .cloned()
            .collect::<HashSet<_>>();

        for field in fields.iter_mut() {
            field.read_at_parse_time = field.attrs.format.is_some()
                || field.attrs.version.is_some()
                || referenced_fields.contains(&field.name);
        }

        Ok(Fields { fields })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }
}

impl Field {
    pub(crate) fn type_for_record(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ } | FieldType::Scalar { typ } => quote!(BigEndian<#typ>),
            _ => panic!("arrays and custom types not supported in records"),
        }
    }

    pub(crate) fn shape_byte_range_fn_name(&self) -> syn::Ident {
        quote::format_ident!("{}_byte_range", &self.name)
    }

    fn shape_byte_len_field_name(&self) -> syn::Ident {
        quote::format_ident!("{}_byte_len", &self.name)
    }

    fn shape_byte_start_field_name(&self) -> syn::Ident {
        // used when fields are optional
        quote::format_ident!("{}_byte_start", &self.name)
    }

    #[allow(dead_code)]
    fn is_array(&self) -> bool {
        matches!(&self.typ, FieldType::Array { .. })
    }

    fn has_computed_len(&self) -> bool {
        self.attrs.len.is_some() || self.attrs.count.is_some()
    }

    fn is_version_dependent(&self) -> bool {
        self.attrs.available.is_some()
    }

    fn validate_at_parse(&self) -> bool {
        false
        //FIXME: validate fields?
        //self.attrs.format.is_some()
    }

    fn has_getter(&self) -> bool {
        self.attrs.no_getter.is_none()
    }

    fn len_expr(&self) -> TokenStream {
        // is this a scalar/offset? then it's just 'RAW_BYTE_LEN'
        // is this computed? then it is stored
        match &self.typ {
            FieldType::Offset { typ } | FieldType::Scalar { typ } => {
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
            .flat_map(|expr| expr.referenced_fields.iter())
            .chain(
                self.attrs
                    .len
                    .as_ref()
                    .into_iter()
                    .flat_map(|expr| expr.referenced_fields.iter()),
            )
    }

    /// the code generated for this field to validate data at parse time.
    fn field_parse_validation_stmts(&self) -> TokenStream {
        let name = &self.name;
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
                let count_expr = &self
                    .attrs
                    .count
                    .as_ref()
                    .expect("must have one of count or len")
                    .expr;
                let inner_type = self.typ.inner_type().expect("only arrays have count attr");
                quote! ( (#count_expr) as usize * #inner_type::RAW_BYTE_LEN )
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

    pub(crate) fn getter_return_type(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ } | FieldType::Scalar { typ } => typ.to_token_stream(),
            FieldType::Other { typ } => quote!( &#typ ),
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ } | FieldType::Scalar { typ } => quote!(&[BigEndian<#typ>]),
                FieldType::Other { typ } => quote!( &[#typ] ),
                _ => unreachable!(),
            },
        }
    }
}

impl FieldType {
    /// 'cooked', as in now 'raw', i.e no 'BigEndian' wrapper
    pub(crate) fn cooked_type_tokens(&self) -> &syn::Ident {
        match &self {
            FieldType::Offset { typ } | FieldType::Scalar { typ } | FieldType::Other { typ } => typ,

            FieldType::Array { .. } => panic!("array tokens never cooked"),
        }
    }

    fn inner_type(&self) -> Option<&syn::Ident> {
        if let FieldType::Array { inner_typ } = &self {
            Some(inner_typ.cooked_type_tokens())
        } else {
            None
        }
    }
}

impl Table {
    pub(crate) fn shape_name(&self) -> syn::Ident {
        quote::format_ident!("{}Shape", &self.name)
    }

    pub(crate) fn iter_shape_byte_fns(&self) -> impl Iterator<Item = TokenStream> + '_ {
        let mut prev_field_end_expr = quote!(0);
        let mut iter = self.fields.iter();

        std::iter::from_fn(move || {
            let field = iter.next()?;
            let fn_name = field.shape_byte_range_fn_name();
            let len_expr = field.len_expr();

            // versioned fields have a different signature
            if field.attrs.available.is_some() {
                prev_field_end_expr = quote!(compile_error!(
                    "non-version dependent field cannot follow version-dependent field"
                ));
                let start_field_name = field.shape_byte_start_field_name();
                return Some(quote! {
                    fn #fn_name(&self) -> Option<Range<usize>> {
                        let start = self.#start_field_name?;
                        Some(start..start + #len_expr)
                    }
                });
            }

            let result = quote! {
                fn #fn_name(&self) -> Range<usize> {
                    let start = #prev_field_end_expr;
                    start..start + #len_expr
                }
            };
            prev_field_end_expr = quote!( self.#fn_name().end );

            Some(result)
        })
    }

    pub(crate) fn iter_shape_fields(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.iter_shape_field_names_and_types()
            .map(|(ident, typ)| quote!( #ident: #typ ))
    }

    pub(crate) fn iter_shape_field_names(&self) -> impl Iterator<Item = syn::Ident> + '_ {
        self.iter_shape_field_names_and_types()
            .map(|(name, _)| name)
    }

    pub(crate) fn iter_shape_field_names_and_types(
        &self,
    ) -> impl Iterator<Item = (syn::Ident, TokenStream)> + '_ {
        let mut iter = self.fields.iter();
        let mut return_me = None;

        // a given field can have 0, 1, or 2 shape fields.
        std::iter::from_fn(move || loop {
            if let Some(thing) = return_me.take() {
                return Some(thing);
            }

            let next = iter.next()?;
            let is_versioned = next.attrs.available.is_some();
            let has_computed_len = next.has_computed_len();
            if !(is_versioned || has_computed_len) {
                continue;
            }

            let start_field = is_versioned.then(|| {
                let field_name = next.shape_byte_start_field_name();
                (field_name, quote!(Option<usize>))
            });

            let len_field = has_computed_len.then(|| {
                let field_name = next.shape_byte_len_field_name();
                if is_versioned {
                    (field_name, quote!(Option<usize>))
                } else {
                    (field_name, quote!(usize))
                }
            });
            if start_field.is_some() {
                return_me = len_field;
                return start_field;
            } else {
                return len_field;
            }
        })
    }

    pub(crate) fn iter_field_validation_stmts(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().map(Field::field_parse_validation_stmts)
    }

    pub(crate) fn iter_table_ref_getters(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields
            .iter()
            .filter(|fld| fld.has_getter())
            .map(|fld| {
                let name = &fld.name;
                let return_type = fld.getter_return_type();
                let shape_range_fn_name = fld.shape_byte_range_fn_name();
                let is_array = fld.is_array();
                let is_versioned = fld.is_version_dependent();
                let read_stmt = if is_array {
                    quote!(self.data.read_array(range).unwrap())
                } else {
                    quote!(self.data.read_at(range.start).unwrap())
                };

                if is_versioned {
                    quote! {
                        pub fn #name(&self) -> Option<#return_type> {
                            let range = self.shape.#shape_range_fn_name()?;
                            Some(#read_stmt)
                        }
                    }
                } else {
                    quote! {
                        pub fn #name(&self) -> #return_type {
                            let range = self.shape.#shape_range_fn_name();
                            // we would like to skip this unwrap
                            #read_stmt
                        }
                    }
                }
            })
    }
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
        Self::new(fields)
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        //let _attrs = get_optional_attributes(input)?;
        let attrs = input.parse()?;
        let name = input.parse::<syn::Ident>()?;
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
            return Ok(FieldType::Other {
                typ: last.ident.clone(),
            });
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

static DOC: &str = "doc";
static NULLABLE: &str = "nullable";
static NO_GETTER: &str = "no_getter";
static COUNT: &str = "count";
static LEN: &str = "len";
static COMPUTE_COUNT: &str = "compute_count";
static AVAILABLE: &str = "available";
static FORMAT: &str = "format";
static VERSION: &str = "version";

impl Parse for FieldAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut this = FieldAttrs::default();
        let attrs = Attribute::parse_outer(input)
            .map_err(|e| syn::Error::new(e.span(), format!("hmm: '{e}'")))?;

        for attr in attrs {
            let ident = attr.path.get_ident().ok_or_else(|| {
                syn::Error::new(attr.path.span(), "attr paths should be a single identifer")
            })?;
            if ident == DOC {
                this.docs.push(attr);
            } else if ident == NULLABLE {
                this.nullable = Some(attr.path);
            } else if ident == NO_GETTER {
                this.no_getter = Some(attr.path);
            } else if ident == VERSION {
                this.version = Some(attr.path);
            } else if ident == COUNT {
                this.count = Some(parse_inline_expr(attr.tokens)?);
            } else if ident == AVAILABLE {
                this.available = Some(attr.parse_args()?);
            } else if ident == COMPUTE_COUNT {
                //this.comp
            } else if ident == LEN {
                this.len = Some(parse_inline_expr(attr.tokens)?);
            } else if ident == FORMAT {
                this.format = Some(FormatAttr {
                    kw: ident.clone(),
                    value: parse_attr_eq_value(attr.tokens)?,
                });
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("unknown field attribute {ident}"),
                ));
            }
        }
        Ok(this)
    }
}

fn parse_inline_expr(tokens: TokenStream) -> syn::Result<InlineExpr> {
    let s = tokens.to_string();
    let mut idents = Vec::new();
    let find_dollar_idents = regex::Regex::new(r#"(\$) (\w+)"#).unwrap();
    for ident in find_dollar_idents.captures_iter(&s) {
        let text = ident.get(2).unwrap().as_str();
        let ident = syn::parse_str::<syn::Ident>(text)
            .map_err(|_| syn::Error::new(tokens.span(), format!("invalid ident '{text}'")))?;
        idents.push(ident);
    }
    let expr: syn::Expr = if idents.is_empty() {
        syn::parse2(tokens)
    } else {
        let new_source = find_dollar_idents.replace_all(&s, "$2");
        syn::parse_str(&new_source)
    }?;

    idents.sort_unstable();
    idents.dedup();

    Ok(InlineExpr {
        expr,
        referenced_fields: idents,
    })
}

fn parse_attr_eq_value<T: Parse>(tokens: TokenStream) -> syn::Result<T> {
    /// the tokens '= T' where 'T' is any `Parse`
    struct EqualsThing<T>(T);

    impl<T: Parse> Parse for EqualsThing<T> {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            input.parse::<Token![=]>()?;
            input.parse().map(EqualsThing)
        }
    }
    syn::parse2::<EqualsThing<T>>(tokens).map(|t| t.0)
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

//fn make_resolved_ident(ident: &syn::Ident) -> syn::Ident {
//quote::format_ident!("__resolved_{}", ident)
//}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use quote::ToTokens;

    use super::*;

    #[test]
    fn parse_inline_expr_simple() {
        let s = "div_me($hi * 5)";
        let hmm = TokenStream::from_str(s).unwrap();
        let inline = super::parse_inline_expr(hmm).unwrap();
        assert_eq!(inline.referenced_fields.len(), 1);
        assert_eq!(
            inline.expr.into_token_stream().to_string(),
            "div_me (hi * 5)"
        );
    }

    #[test]
    fn parse_inline_expr_dedup() {
        let s = "div_me($hi * 5 + $hi)";
        let hmm = TokenStream::from_str(s).unwrap();
        let inline = super::parse_inline_expr(hmm).unwrap();
        assert_eq!(inline.referenced_fields.len(), 1);
        assert_eq!(
            inline.expr.into_token_stream().to_string(),
            "div_me (hi * 5 + hi)"
        );
    }
}
