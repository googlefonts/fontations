#![allow(dead_code)]

use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token, Attribute, Token,
};

pub struct Item {
    pub attrs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub fields: Vec<Field>,
}

pub struct Field {
    pub attrs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub ty: syn::Type,
}

pub enum Type {
    Array(syn::Ident),
    Single(syn::Ident),
    // not sure we want to have full paths here? I think everything should need
    // to be in scope.
    //Path(syn::Path),
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(&input)?;
        let name: syn::Ident = input.parse()?;
        let content;
        let _ = braced!(content in input);
        let fields = Punctuated::<Field, Token![,]>::parse_terminated(&content)?;
        let fields = fields.into_iter().collect();
        Ok(Self {
            attrs,
            name,
            fields,
        })
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(&input)?;
        let name = input.parse()?;
        let _ = input.parse::<Token![:]>()?;
        let ty = input.parse()?;

        Ok(Field { attrs, name, ty })
    }
}

impl Parse for Type {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        if input.lookahead1().peek(token::Bracket) {
            let content;
            bracketed!(content in input);
            return Ok(Type::Array(content.parse()?));
        }

        input.parse().map(Type::Single)
    }
}

fn get_optional_attributes(input: &ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    Ok(input
        .lookahead1()
        .peek(Token![#])
        .then(|| Attribute::parse_outer(&input))
        .transpose()?
        .unwrap_or_default())
}
