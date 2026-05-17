//! Field and field-type parsing.

use std::hash::Hash;

use log::debug;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Token,
};

use super::{logged_syn_error, FieldAttrs, TableReadArgs};

#[derive(Debug, Clone)]
pub(crate) struct Fields {
    // not parsed, but set when the table/record is parsed
    pub(crate) read_args: Option<TableReadArgs>,
    pub(crate) fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub(crate) attrs: FieldAttrs,
    pub(crate) name: syn::Ident,
    pub(crate) typ: FieldType,
    /// `true` if the presence of this field is guaranteed if the containing
    /// table parses successfully.
    ///
    /// This is true for fields at the start of a table, up to the first conditional.
    ///
    /// These fields must be present, which means reads can unwrap (and could even
    /// be unsafe.)
    pub(crate) validated_at_parse: bool,
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

impl OffsetTarget {
    pub(crate) fn getter_return_type(&self, is_generic: bool) -> TokenStream {
        match self {
            OffsetTarget::Table(ident) if !is_generic => quote!(Result<#ident <'a>, ReadError>),
            OffsetTarget::Table(ident) => quote!(Result<#ident, ReadError>),
            OffsetTarget::Array(inner) => {
                let elem_type = match std::ops::Deref::deref(inner) {
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

// ── Parse impls ─────────────────────────────────────────────────────

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
            validated_at_parse: false,
        })
    }
}

impl Parse for FieldType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let type_: syn::Type = input.parse()?;
        Self::from_syn_type(&type_)
    }
}

// ── FieldType resolution ────────────────────────────────────────────

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

pub(super) fn resolve_ident<'a>(
    known: &'a std::collections::HashMap<syn::Ident, FieldType>,
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

pub(super) fn resolve_field(
    known: &std::collections::HashMap<syn::Ident, FieldType>,
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

// ── Helper types and functions ──────────────────────────────────────

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

impl std::str::FromStr for WellKnownScalar {
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
        std::str::FromStr::from_str(path.ident.to_string().as_str())
    }
}

fn get_single_path_segment(path: &syn::Path) -> syn::Result<&syn::PathSegment> {
    if path.segments.len() != 1 {
        return Err(logged_syn_error(path.span(), "expect a single-item path"));
    }
    Ok(path.segments.last().unwrap())
}

// either a single ident or an array
pub(super) fn get_offset_target(input: &syn::PathSegment) -> syn::Result<OffsetTarget> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_path_seg(s: &str) -> syn::PathSegment {
        let path = syn::parse_str::<syn::Path>(s).unwrap();
        path.segments.last().unwrap().clone()
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
}
