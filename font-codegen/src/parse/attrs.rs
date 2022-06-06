use quote::{quote_spanned, ToTokens};
use syn::{spanned::Spanned, Lit};

use super::{ArrayField, SingleField};

/// All of the attrs that can be applied to a field.
///
/// These are not validated, and do not all make sense on all fields;
/// rather they are just collected here.
#[derive(Default)]
pub struct FieldAttrs {
    docs: Vec<syn::Attribute>,
    hidden: Option<syn::Path>,
    no_getter: Option<syn::Path>,
    count: Option<Count>,
    variable_size: Option<syn::Path>,
}

/// Annotations for how to calculate the count of an array.
pub enum Count {
    Field(syn::Ident),
    Literal(syn::LitInt),
    All(syn::Path),
    Function {
        fn_: syn::Path,
        args: Vec<syn::Ident>,
    },
}

#[derive(Default)]
pub struct VariantAttrs {
    pub docs: Vec<syn::Attribute>,
    pub version: Option<Version>,
}

/// Used to specify the version/format specifier for an enum variant
pub enum Version {
    Lit(syn::LitInt),
    /// A path to a constant to be matched against
    Const(syn::Path),
    /// a path to a method which should return `true` for the first match
    With(syn::Path),
}

#[derive(Default)]
pub struct ItemAttrs {
    pub docs: Vec<syn::Attribute>,
    pub format: Option<syn::Ident>,
    pub generate_getters: Option<syn::Path>,
    pub offset_host: Option<syn::Path>,
    pub init: Vec<(syn::Ident, syn::Type)>,
    pub repr: Option<syn::Ident>,
    pub flags: Option<syn::Ident>,
}

const NO_GETTER: &str = "no_getter";

impl FieldAttrs {
    pub fn parse(attrs: &[syn::Attribute]) -> Result<FieldAttrs, syn::Error> {
        let mut result = FieldAttrs::default();
        for attr in attrs {
            match attr.parse_meta()? {
                syn::Meta::NameValue(value) if value.path.is_ident("doc") => {
                    result.docs.push(attr.clone());
                }
                syn::Meta::Path(path) if path.is_ident("hidden") => {
                    result.hidden = Some(path.clone())
                }
                syn::Meta::Path(path) if path.is_ident(NO_GETTER) => {
                    result.no_getter = Some(path.clone())
                }

                syn::Meta::Path(path) if path.is_ident("variable_size") => {
                    result.variable_size = Some(path.clone())
                }
                syn::Meta::Path(path) if path.is_ident("count_all") => {
                    result.count = Some(Count::All(path.clone()));
                }

                syn::Meta::List(list) if list.path.is_ident("count") => {
                    let inner = expect_single_item_list(&list)?;
                    match inner {
                        syn::NestedMeta::Meta(syn::Meta::Path(p)) if p.get_ident().is_some() => {
                            result.count = Some(Count::Field(p.get_ident().unwrap().clone()));
                        }
                        syn::NestedMeta::Lit(syn::Lit::Int(int)) => {
                            result.count = Some(Count::Literal(int));
                        }
                        _ => return Err(syn::Error::new(
                            list.path.span(),
                            "count attribute should have format #[count(field)] or #[count(123)]",
                        )),
                    }
                }
                syn::Meta::List(list) if list.path.is_ident("count_with") => {
                    let mut items = list.nested.iter();
                    if let Some(syn::NestedMeta::Meta(syn::Meta::Path(path))) = items.next() {
                        let args = items.map(expect_ident).collect::<Result<_, _>>()?;
                        assert!(result.count.is_none(), "I ONLY COUNT ONCE");
                        result.count = Some(Count::Function {
                            fn_: path.to_owned(),
                            args,
                        });
                        continue;
                    }
                    return Err(syn::Error::new(
                        list.path.span(),
                        "count_with attribute should have format count_with(path::to::fn, arg1, arg2)",
                    ));
                }
                other => {
                    return Err(syn::Error::new(other.span(), "unknown attribute"));
                }
            }
        }
        Ok(result)
    }

    pub fn into_array(
        self,
        name: syn::Ident,
        inner_typ: syn::Path,
        inner_lifetime: Option<syn::Lifetime>,
    ) -> Result<ArrayField, syn::Error> {
        if let Some(path) = &self.hidden {
            return Err(syn::Error::new(
                path.span(),
                "'hidden' is only valid on scalar fields",
            ));
        }
        let count = self.count.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "array types require 'count' or 'count_with' attribute",
            )
        })?;
        Ok(ArrayField {
            docs: self.docs,
            name,
            inner_typ,
            inner_lifetime,
            count,
            variable_size: self.variable_size,
            no_getter: self.no_getter,
        })
    }

    pub fn into_single(self, name: syn::Ident, typ: syn::Path) -> Result<SingleField, syn::Error> {
        if let Some(span) = self.count.as_ref().map(Count::span) {
            return Err(syn::Error::new(
                span,
                "count/count_with attribute not valid on scalar fields",
            ));
        }
        if let Some(token) = self.variable_size {
            return Err(syn::Error::new(token.span(), "not valid on scalar fields"));
        }

        Ok(SingleField {
            docs: self.docs,
            name,
            typ,
            hidden: self.hidden,
        })
    }
}

impl Count {
    fn span(&self) -> proc_macro2::Span {
        match self {
            Count::All(path) => path.span(),
            Count::Field(ident) => ident.span(),
            Count::Function { fn_, .. } => fn_.span(),
            Count::Literal(lit) => lit.span(),
        }
    }

    pub fn iter_input_fields(&self) -> impl Iterator<Item = &syn::Ident> {
        let fn_fields = match self {
            Count::Function { args, .. } => args.as_slice(),
            _ => &[],
        };

        let field = match self {
            Count::Field(ident) => Some(ident),
            _ => None,
        };

        field.into_iter().chain(fn_fields)
    }
}

static VERSION: &str = "version";
static VERSION_WITH: &str = "version_with";

impl VariantAttrs {
    pub fn parse(attrs: &[syn::Attribute]) -> Result<VariantAttrs, syn::Error> {
        let mut result = VariantAttrs::default();
        for attr in attrs {
            match attr.parse_meta()? {
                syn::Meta::NameValue(value) if value.path.is_ident("doc") => {
                    result.docs.push(attr.clone());
                }
                syn::Meta::List(list) if list.path.is_ident(VERSION) => {
                    let item = expect_single_item_list(&list)?;
                    result.version = match item {
                        syn::NestedMeta::Meta(syn::Meta::Path(p)) => {
                            Some(Version::Const(p.clone()))
                        }
                        syn::NestedMeta::Lit(syn::Lit::Int(lit)) => Some(Version::Lit(lit)),
                        _ => {
                            return Err(syn::Error::new(
                                list.path.span(),
                                "expected integer literal or path to constant",
                            ))
                        }
                    };
                }
                syn::Meta::List(list) if list.path.is_ident(VERSION_WITH) => {
                    let inner = expect_single_item_list(&list)?;
                    if let syn::NestedMeta::Meta(syn::Meta::Path(path)) = inner {
                        result.version = Some(Version::With(path));
                    } else {
                        return Err(syn::Error::new(inner.span(), "expected path to method"));
                    }
                }
                other => return Err(syn::Error::new(other.span(), "unknown attribute")),
            }
        }
        Ok(result)
    }
}

static FORMAT: &str = "format";
static REPR: &str = "repr";
static FLAGS: &str = "flags";
static OFFSET_HOST: &str = "offset_host";
static GENERATE_GETTERS: &str = "generate_getters";
static INIT: &str = "init";

impl ItemAttrs {
    pub fn parse(attrs: &[syn::Attribute]) -> Result<ItemAttrs, syn::Error> {
        let mut result = ItemAttrs::default();
        for attr in attrs {
            match attr.parse_meta()? {
                syn::Meta::Path(path) if path.is_ident(OFFSET_HOST) => {
                    result.offset_host = Some(path)
                }
                syn::Meta::Path(path) if path.is_ident(GENERATE_GETTERS) => {
                    result.generate_getters = Some(path)
                }
                syn::Meta::NameValue(value) if value.path.is_ident("doc") => {
                    result.docs.push(attr.clone());
                }
                syn::Meta::List(list) if list.path.is_ident(REPR) => {
                    let item = expect_single_item_list(&list)?;
                    result.repr = Some(expect_ident(&item)?);
                }
                syn::Meta::List(list) if list.path.is_ident(FLAGS) => {
                    let item = expect_single_item_list(&list)?;
                    result.flags = Some(expect_ident(&item)?);
                }
                syn::Meta::List(list) if list.path.is_ident(FORMAT) => {
                    let item = expect_single_item_list(&list)?;
                    result.format = Some(expect_ident(&item)?);
                }
                syn::Meta::List(list) if list.path.is_ident(INIT) => {
                    result.init = list
                        .nested
                        .iter()
                        .map(expect_init_arg)
                        .collect::<Result<_, _>>()?;
                }
                other => return Err(syn::Error::new(other.span(), "unknown attribute")),
            }
        }
        Ok(result)
    }
}

fn expect_single_item_list(meta: &syn::MetaList) -> Result<syn::NestedMeta, syn::Error> {
    match meta.nested.first() {
        Some(item) if meta.nested.len() == 1 => Ok(item.clone()),
        _ => Err(syn::Error::new(meta.span(), "expected single item list")),
    }
}

fn expect_init_arg(meta: &syn::NestedMeta) -> Result<(syn::Ident, syn::Type), syn::Error> {
    match meta {
        syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue))
            if namevalue.path.get_ident().is_some() =>
        {
            let name = namevalue.path.get_ident().unwrap();
            if let Lit::Str(s) = &namevalue.lit {
                let typ: syn::Type = syn::parse_str(s.value().trim_matches('"'))?;
                Ok((name.clone(), typ))
            } else {
                Err(syn::Error::new(
                    namevalue.lit.span(),
                    "type must be a string literal (e.g.: 'name = \"usize\"')",
                ))
            }
        }
        _ => Err(syn::Error::new(meta.span(), "expected 'name = type'")),
    }
}

fn expect_ident(meta: &syn::NestedMeta) -> Result<syn::Ident, syn::Error> {
    match meta {
        syn::NestedMeta::Meta(syn::Meta::Path(p)) if p.get_ident().is_some() => {
            Ok(p.get_ident().unwrap().clone())
        }
        _ => Err(syn::Error::new(meta.span(), "expected ident")),
    }
}

impl Version {
    pub fn const_version_tokens(&self) -> Option<&syn::Path> {
        match self {
            Version::Const(path) => Some(path),
            _ => None,
        }
    }
}

impl ToTokens for Version {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        match self {
            Version::Lit(lit) => lit.to_tokens(stream),
            Version::Const(path) => path.to_tokens(stream),
            Version::With(path) => {
                let span = path.span();
                stream.extend(quote_spanned!(span=> v if #path(v)))
            }
        }
    }
}
