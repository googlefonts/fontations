use syn::{spanned::Spanned, Meta, MetaList, NestedMeta};

const TOP_ATTR: &str = "font_thing";
const COUNT_ATTR: &str = "count";
const DATA_ATTR: &str = "data";

pub struct Field {
    pub vis: syn::Visibility,
    pub name: syn::Ident,
    pub ty: syn::Path,
    pub attrs: Option<Attrs>,
}

#[derive(Clone, Default)]
pub struct Attrs {
    pub data: Option<syn::Path>,
    pub count: Option<syn::Ident>,
    pub count_fn: Option<CountFn>,
    pub count_all: Option<syn::Path>,
}

#[derive(Clone)]
pub struct CountFn {
    pub fn_: syn::Path,
    pub args: Vec<syn::Ident>,
}

impl Field {
    pub(crate) fn parse(field: &syn::Field) -> Result<Self, syn::Error> {
        let name = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new(field.span(), "only named fields are supported"))?;
        let ty = match &field.ty {
            syn::Type::Path(p) if p.qself.is_none() => p.path.clone(),
            _ => {
                return Err(syn::Error::new(
                    field.ty.span(),
                    "field can only contain named, unqualified types",
                ))
            }
        };
        let vis = field.vis.clone();

        let attr = match field.attrs.iter().find(|attr| attr.path.is_ident(TOP_ATTR)) {
            Some(attr) => attr,
            None => {
                return Ok(Field {
                    name,
                    ty,
                    vis,
                    attrs: None,
                })
            }
        };

        let attrs = Attrs::parse(attr)?;
        Ok(Field {
            name,
            ty,
            vis,
            attrs: Some(attrs),
        })
    }
}

impl Attrs {
    fn parse(attr: &syn::Attribute) -> Result<Self, syn::Error> {
        let meta = match attr.parse_meta()? {
            syn::Meta::List(list) => list,
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "Expected attribute list in (#[font_thing(one = \"value\")])",
                ))
            }
        };

        let mut this = Attrs::default();
        for item in meta.nested.iter() {
            match item {
                NestedMeta::Meta(Meta::NameValue(val)) if val.path.is_ident(COUNT_ATTR) => {
                    this.count = Some(expect_lit_str(&val.lit).and_then(str_to_ident)?);
                }
                NestedMeta::Meta(Meta::List(list)) if list.path.is_ident(COUNT_ATTR) => {
                    this.count_fn = Some(parse_count_fn(list)?);
                }
                NestedMeta::Meta(Meta::Path(val)) if val.is_ident("all") => {
                    this.count_all = Some(val.clone());
                }
                NestedMeta::Meta(Meta::Path(val)) if val.is_ident(DATA_ATTR) => {
                    this.data = Some(val.to_owned());
                }
                _ => return Err(syn::Error::new(item.span(), "unknown attribute")),
            }
        }
        Ok(this)
    }
}

fn parse_count_fn(list: &MetaList) -> Result<CountFn, syn::Error> {
    let mut count_fn = None;
    let mut parsed_args = Vec::new();

    for item in list.nested.iter() {
        match item {
            NestedMeta::Meta(Meta::NameValue(val)) if val.path.is_ident("fn") => {
                if count_fn.is_some() {
                    return Err(syn::Error::new(val.path.span(), "duplicate attribute"));
                }
                let path_str = expect_lit_str(&val.lit)?;
                let path: syn::Path = syn::parse_str(&path_str.value())?;
                count_fn = Some(path);
            }
            NestedMeta::Meta(Meta::List(args)) if args.path.is_ident("args") => {
                if args.nested.is_empty() {
                    return Err(syn::Error::new(args.span(), "args should not be empty"));
                }
                for arg in args.nested.iter() {
                    match arg {
                        NestedMeta::Lit(lit) => {
                            let lit_str = expect_lit_str(lit).and_then(str_to_ident)?;
                            parsed_args.push(lit_str);
                        }
                        _ => {
                            return Err(syn::Error::new(
                                arg.span(),
                                "unexpected item, expected (string) arguments",
                            ));
                        }
                    }
                }
            }
            _ => return Err(syn::Error::new(item.span(), "unexpected attribute")),
        }
    }

    if count_fn.is_none() {
        return Err(syn::Error::new(
            list.span(),
            "missing required argument 'fn'",
        ));
    }

    count_fn
        .map(|fn_| CountFn {
            fn_,
            args: parsed_args,
        })
        .ok_or_else(|| syn::Error::new(list.span(), "missing required argument 'fn'"))
}

fn expect_lit_str(lit: &syn::Lit) -> Result<syn::LitStr, syn::Error> {
    match lit {
        syn::Lit::Str(s) => Ok(s.clone()),
        _ => Err(syn::Error::new(lit.span(), "expected string literal")),
    }
}

fn str_to_ident(lit: syn::LitStr) -> Result<syn::Ident, syn::Error> {
    Ok(syn::Ident::new(&lit.value(), lit.span()))
}
