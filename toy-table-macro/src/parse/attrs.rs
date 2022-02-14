use syn::spanned::Spanned;

use super::{ArrayField, ScalarField, ScalarType};

#[derive(Default)]
pub struct AllAttrs {
    hidden: Option<syn::Path>,
    count: Option<Count>,
    variable_size: Option<syn::Path>,
}

/// Annotations for how to calculate the count of an array.
pub enum Count {
    Field(syn::Ident),
    Function {
        fn_: syn::Path,
        args: Vec<syn::Ident>,
    },
}

impl AllAttrs {
    pub fn parse(attrs: &[syn::Attribute]) -> Result<AllAttrs, syn::Error> {
        let mut result = AllAttrs::default();
        for attr in attrs {
            //dbg!(attr);
            match attr.parse_meta()? {
                syn::Meta::Path(path) if path.is_ident("hidden") => {
                    result.hidden = Some(path.clone())
                }
                syn::Meta::Path(path) if path.is_ident("variable_size") => {
                    result.variable_size = Some(path.clone())
                }
                syn::Meta::List(list) if list.path.is_ident("count") => {
                    if let Some(syn::NestedMeta::Meta(syn::Meta::Path(p))) = list.nested.first() {
                        if let Some(ident) = p.get_ident() {
                            result.count = Some(Count::Field(ident.clone()));
                            continue;
                        }
                    }
                    return Err(syn::Error::new(
                        list.path.span(),
                        "count attribute should have format count(some_path)",
                    ));
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
                other => return Err(syn::Error::new(other.span(), "unknown attribute")),
            }
        }
        Ok(result)
    }

    pub fn into_array(
        self,
        name: syn::Ident,
        inner_typ: syn::Ident,
        inner_lifetime: bool,
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
        let variable_size = self.variable_size;
        Ok(ArrayField {
            name,
            inner_typ,
            inner_lifetime,
            count,
            variable_size,
        })
    }

    pub fn into_scalar(self, name: syn::Ident, typ: ScalarType) -> Result<ScalarField, syn::Error> {
        if let Some(span) = self.count.as_ref().map(Count::span) {
            return Err(syn::Error::new(
                span,
                "count/count_with attribute not valid on scalar fields",
            ));
        }
        if let Some(token) = self.variable_size {
            return Err(syn::Error::new(token.span(), "not valid on scalar fields"));
        }

        Ok(ScalarField {
            name,
            typ,
            hidden: self.hidden,
        })
    }
}

impl Count {
    fn span(&self) -> proc_macro2::Span {
        match self {
            Count::Field(ident) => ident.span(),
            Count::Function { fn_, .. } => fn_.span(),
        }
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
