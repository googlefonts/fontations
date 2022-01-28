use syn::{spanned::Spanned, Meta, NestedMeta};

const TOP_ATTR: &str = "font_thing";
const COUNT_ATTR: &str = "count";
const OFFSET_ATTR: &str = "offset";
//const ARGS_ATTR: &str = "args";

pub struct Field {
    pub name: syn::Ident,
    pub ty: syn::Type,
    pub attrs: Option<Attrs>,
}

#[derive(Debug, Clone, Default)]
pub struct Attrs {
    count: Option<String>,
    offset: Option<String>,
    _args: Vec<String>,
}

impl Field {
    pub(crate) fn parse(field: &syn::Field) -> Result<Self, syn::Error> {
        let name = field
            .ident
            .clone()
            //.map(|id| id.to_string().trim_start_matches("r#").to_owned())
            .ok_or_else(|| syn::Error::new(field.span(), "only named fields are supported"))?;
        let ty = field.ty.clone();

        let attr = match field.attrs.iter().find(|attr| attr.path.is_ident(TOP_ATTR)) {
            Some(attr) => attr,
            None => {
                return Ok(Field {
                    name,
                    ty,
                    attrs: None,
                })
            }
        };

        let attrs = Attrs::parse(&attr)?;
        Ok(Field {
            name,
            ty,
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
                    this.count = Some(expect_lit_str(&val.lit)?);
                }
                NestedMeta::Meta(Meta::NameValue(val)) if val.path.is_ident(OFFSET_ATTR) => {
                    this.offset = Some(expect_lit_str(&val.lit)?);
                }
                _ => return Err(syn::Error::new(item.span(), "unknown attribute")),
            }
        }
        Ok(this)
    }
}

fn expect_lit_str(lit: &syn::Lit) -> Result<String, syn::Error> {
    match lit {
        syn::Lit::Str(s) => Ok(s.value()),
        _ => Err(syn::Error::new(lit.span(), "expected string literal")),
    }
}
