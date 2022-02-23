use quote::{quote, quote_spanned};
use syn::{
    braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token, Attribute, Token,
};

mod attrs;

pub use attrs::Count;
use attrs::{FieldAttrs, VariantAttrs};

use self::attrs::ItemAttrs;

pub struct Items(Vec<Item>);

pub enum Item {
    Single(SingleItem),
    Group(ItemGroup),
    RawEnum(RawEnum),
}

/// A single concrete object, such as a particular table version or record format.
pub struct SingleItem {
    pub docs: Vec<syn::Attribute>,
    pub lifetime: Option<syn::Lifetime>,
    pub offset_host: Option<syn::Path>,
    pub name: syn::Ident,
    pub fields: Vec<Field>,
}

/// A group of items that can exist in the same location, such as tables
/// with multiple versions.
pub struct ItemGroup {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub lifetime: Option<syn::Lifetime>,
    pub format_typ: syn::Ident,
    pub variants: Vec<Variant>,
}

/// A raw c-style enum
pub struct RawEnum {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub repr: syn::Ident,
    pub variants: Vec<RawVariant>,
}

/// A raw scalar variant
pub struct RawVariant {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub value: syn::LitInt,
}

pub struct Variant {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub version: attrs::Version,
    pub typ: syn::Ident,
    pub typ_lifetime: Option<syn::Lifetime>,
}

pub enum Field {
    Single(SingleField),
    Array(ArrayField),
}

pub struct SingleField {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub typ: syn::Path,
    pub hidden: Option<syn::Path>,
}

pub struct ArrayField {
    pub docs: Vec<syn::Attribute>,
    pub name: syn::Ident,
    pub inner_typ: syn::Path,
    pub inner_lifetime: Option<syn::Lifetime>,
    pub count: Count,
    pub variable_size: Option<syn::Path>,
}

impl Parse for Items {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let mut result = Vec::new();
        while !input.is_empty() {
            result.push(input.parse()?)
        }
        Ok(Self(result))
    }
}

impl Items {
    pub fn iter(&self) -> impl Iterator<Item = &Item> {
        self.0.iter()
    }
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = ItemAttrs::parse(&attrs)?;
        let enum_token = input
            .peek(Token![enum])
            .then(|| input.parse::<Token![enum]>())
            .transpose()?;
        let name: syn::Ident = input.parse()?;
        let lifetime = validate_lifetime(input)?;
        let content;
        let _ = braced!(content in input);
        if enum_token.is_some() && attrs.repr.is_some() {
            let variants = Punctuated::<RawVariant, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            RawEnum::new(name, variants, attrs).map(Self::RawEnum)
        } else if enum_token.is_some() {
            let variants = Punctuated::<Variant, Token![,]>::parse_terminated(&content)?;
            let variants = variants.into_iter().collect();
            ItemGroup::new(name, lifetime, variants, attrs).map(Self::Group)
        } else {
            let fields = Punctuated::<Field, Token![,]>::parse_terminated(&content)?;
            let fields = fields.into_iter().collect();
            let item = SingleItem {
                docs: attrs.docs,
                offset_host: attrs.offset_host,
                lifetime,
                name,
                fields,
            };
            item.validate()?;
            Ok(Self::Single(item))
        }
    }
}

impl Parse for Variant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = VariantAttrs::parse(&attrs)?;
        let name = input.parse::<syn::Ident>()?;
        let content;
        parenthesized!(content in input);
        let typ = content.parse::<syn::Ident>()?;
        let typ_lifetime = validate_lifetime(&content)?;
        if let Some(version) = attrs.version {
            Ok(Self {
                docs: attrs.docs,
                name,
                version,
                typ,
                typ_lifetime,
            })
        } else {
            Err(syn::Error::new(
                name.span(),
                "all variants require #[version(..)] attribute",
            ))
        }
    }
}

impl Parse for RawVariant {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = VariantAttrs::parse(&attrs)?;
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![=]>()?;
        let value: syn::LitInt = input.parse()?;
        Ok(Self {
            docs: attrs.docs,
            name,
            value,
        })
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let attrs = get_optional_attributes(input)?;
        let attrs = FieldAttrs::parse(&attrs)?;
        let name = input.parse::<syn::Ident>()?;
        let _ = input.parse::<Token![:]>()?;

        if input.lookahead1().peek(token::Bracket) {
            let content;
            bracketed!(content in input);
            let typ = content.parse::<syn::Path>()?;
            let lifetime = ensure_single_lifetime(&typ)?;
            attrs.into_array(name, typ, lifetime).map(Field::Array)
        } else {
            attrs.into_single(name, input.parse()?).map(Field::Single)
        }
    }
}

impl Field {
    pub fn name(&self) -> &syn::Ident {
        match self {
            Field::Array(v) => &v.name,
            Field::Single(v) => &v.name,
        }
    }

    pub fn as_single(&self) -> Option<&SingleField> {
        match self {
            Field::Array(_) => None,
            Field::Single(v) => Some(v),
        }
    }

    pub fn as_array(&self) -> Option<&ArrayField> {
        match self {
            Field::Array(v) => Some(v),
            Field::Single(_) => None,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Field::Array(_))
    }

    fn requires_lifetime(&self) -> bool {
        self.is_array()
    }

    pub fn is_single(&self) -> bool {
        matches!(self, Field::Single(_))
    }

    pub fn docs(&self) -> &[syn::Attribute] {
        match self {
            Field::Array(v) => &v.docs,
            Field::Single(v) => &v.docs,
        }
    }

    pub fn view_init_expr(&self) -> proc_macro2::TokenStream {
        let name = self.name();
        let span = name.span();
        let init_fn = match self {
            Field::Single(value) => {
                let typ = &value.typ;
                quote_spanned!(span=> zerocopy::LayoutVerified::<_, #typ>::new_unaligned_from_prefix(bytes)?)
            }
            Field::Array(array) if array.variable_size.is_none() => {
                let typ = &array.inner_typ;
                let count = match &array.count {
                    Count::Field(name) => {
                        let span = name.span();
                        let resolved_value = super::make_resolved_ident(name);
                        Some(quote_spanned!(span=> #resolved_value as usize))
                    }
                    Count::Literal(lit) => {
                        let span = lit.span();
                        Some(quote_spanned!(span=> #lit))
                    }
                    Count::Function { fn_, args } => {
                        let span = fn_.span();
                        let args = args.iter().map(super::make_resolved_ident);
                        Some(quote_spanned!(span=> #fn_( #( #args ),* )))
                    }
                    Count::All(_) => None,
                };
                if let Some(count) = count {
                    quote_spanned!(span=> zerocopy::LayoutVerified::<_, [#typ]>::new_slice_unaligned_from_prefix(bytes, #count)?)
                } else {
                    quote_spanned!(span => (zerocopy::LayoutVerified::<_, [#typ]>::new_slice_unaligned(bytes)?, 0))
                }
            }
            _ => quote_spanned!(span=> compile_errror!("we don't init this type yet")),
        };
        quote_spanned!(span=> let (#name, bytes) = #init_fn;)
    }

    pub fn view_getter_fn(&self) -> Option<proc_macro2::TokenStream> {
        let docs = self.docs();
        let name = self.name();
        let span = name.span();
        match self {
            Field::Single(s) if s.hidden.is_some() => None,
            Field::Array(a) if a.variable_size.is_some() => None,
            _ => {
                let body = self.getter_body();
                let return_type = self.getter_return_type();
                Some(quote_spanned! {span=>
                    #( #docs )*
                    pub fn #name(&self) -> #return_type {
                        #body
                    }
                })
            }
        }
    }

    fn getter_return_type(&self) -> proc_macro2::TokenStream {
        match self {
            Field::Single(field) => field.cooked_type_tokens(),
            Field::Array(array) => {
                let typ = &array.inner_typ;
                let span = array.name.span();
                quote_spanned!(span=> &[#typ])
            }
        }
    }

    fn getter_body(&self) -> proc_macro2::TokenStream {
        let span = self.name().span();
        let name = self.name();
        match self {
            Field::Single(field) if field.is_be_wrapper() => {
                quote_spanned!(span=> self.#name.read().get())
            }
            _ => quote_spanned!(span=> &self.#name),
        }
    }

    /// The type that represents this field in a view struct.
    pub fn view_field_decl(&self) -> proc_macro2::TokenStream {
        let name = self.name();
        match self {
            Field::Single(scalar) => {
                let typ = &scalar.typ;
                let span = typ.span();
                quote_spanned!(span=> #name: zerocopy::LayoutVerified<&'a [u8], #typ>)
            }
            Field::Array(array) if array.variable_size.is_none() => {
                let typ = &array.inner_typ;
                let span = typ.span();
                quote_spanned!(span=> #name: zerocopy::LayoutVerified<&'a [u8], [#typ]>)
            }
            _ => panic!("variable arrays are not handled yet, you shouldn't be calling this"),
        }
    }
}

impl SingleField {
    pub fn is_be_wrapper(&self) -> bool {
        matches!(self.typ.segments.last(), Some(seg) if seg.ident == "BigEndian")
    }

    /// The return type of a getter of this type.
    ///
    /// (this is about returning T for BigEndian<T>)
    pub fn cooked_type_tokens(&self) -> proc_macro2::TokenStream {
        let last = self.typ.segments.last().unwrap();
        if last.ident == "BigEndian" {
            let args = match &last.arguments {
                syn::PathArguments::AngleBracketed(args) => args,
                _ => panic!("BigEndian type should always have generic params"),
            };
            let last_arg = args.args.last().unwrap();
            if let syn::GenericArgument::Type(inner) = last_arg {
                return quote!(#inner);
            }
            panic!("failed to find BigEndian generic type");
        }

        let typ = &self.typ;
        quote!(#typ)
    }
}

impl SingleItem {
    fn validate(&self) -> Result<(), syn::Error> {
        // check for lifetime
        let needs_lifetime = self.fields.iter().any(|x| x.requires_lifetime());
        if needs_lifetime && self.lifetime.is_none() {
            let msg = format!(
                "object containing array or offset requires lifetime param ({}<'a>)",
                self.name
            );
            return Err(syn::Error::new(self.name.span(), &msg));
        } else if !needs_lifetime && self.lifetime.is_some() {
            return Err(syn::Error::new(
                self.lifetime.as_ref().unwrap().span(),
                "only objects containing arrays or offsets require lifetime",
            ));
        }

        // check that fields referenced in #count annotations are sane
        for (field_idx, ident) in self
            .fields
            .iter()
            .enumerate()
            .filter_map(|(i, fld)| fld.as_array().map(|arr| (i, arr)))
            .flat_map(|(i, x)| x.count.iter_input_fields().map(move |id| (i, id)))
        {
            match self.fields.iter().position(|fld| fld.name() == ident) {
                Some(x) if x < field_idx => (),
                Some(_) => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "field must occur before it can be referenced",
                    ))
                }
                None => return Err(syn::Error::new(ident.span(), "unknown field")),
            }
        }

        // ensure #[count_all] is last, if it exists
        for (i, field) in self.fields.iter().enumerate() {
            if let Some(array) = field.as_array() {
                if let Count::All(all) = &array.count {
                    if i != self.fields.len() - 1 {
                        return Err(syn::Error::new(
                            all.span(),
                            "#[count_all] only valid on last item",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

impl ItemGroup {
    fn new(
        name: syn::Ident,
        lifetime: Option<syn::Lifetime>,
        variants: Vec<Variant>,
        attrs: ItemAttrs,
    ) -> Result<Self, syn::Error> {
        if let Some(format_typ) = attrs.format {
            Ok(Self {
                docs: attrs.docs,
                name,
                lifetime,
                variants,
                format_typ,
            })
        } else {
            Err(syn::Error::new(
                name.span(),
                "all enum groups require #[format(..)] attribute",
            ))
        }
    }
}

impl RawEnum {
    fn new(
        name: syn::Ident,
        variants: Vec<RawVariant>,
        attrs: ItemAttrs,
    ) -> Result<Self, syn::Error> {
        let repr = attrs.repr.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "raw enumerations require repr annotation (like: #[repr(u16)])",
            )
        })?;
        Ok(RawEnum {
            docs: attrs.docs,
            repr,
            variants,
            name,
        })
    }
}

fn get_optional_attributes(input: ParseStream) -> Result<Vec<syn::Attribute>, syn::Error> {
    let mut result = Vec::new();
    while input.lookahead1().peek(Token![#]) {
        result.extend(Attribute::parse_outer(input)?);
    }
    Ok(result)
}

fn ensure_single_lifetime(input: &syn::Path) -> Result<Option<syn::Lifetime>, syn::Error> {
    match input.segments.last().map(|seg| &seg.arguments) {
        Some(syn::PathArguments::AngleBracketed(args)) => {
            let mut iter = args.args.iter().filter_map(|arg| match arg {
                syn::GenericArgument::Lifetime(arg) => Some(arg),
                _ => None,
            });
            let result = iter.next();
            if let Some(extra_lifetime) = iter.next() {
                Err(syn::Error::new(
                    extra_lifetime.span(),
                    "at most a single lifetime \"'a\" is supported",
                ))
            } else {
                Ok(result.cloned())
            }
        }
        Some(syn::PathArguments::Parenthesized(args)) => Err(syn::Error::new(
            args.span(),
            "whatever this is trying to do, we definitely do not support it",
        )),
        None | Some(syn::PathArguments::None) => Ok(None),
    }
}

/// Ensure types have at most a single lifetime param, "'a".
fn validate_lifetime(input: ParseStream) -> Result<Option<syn::Lifetime>, syn::Error> {
    let generics = input.parse::<syn::Generics>()?;
    if generics.const_params().count() + generics.type_params().count() > 1 {
        return Err(syn::Error::new(
            generics.span(),
            "font types should contain at most a single lifetime",
        ));
    }
    if let Some(lifetime) = generics.lifetimes().nth(1) {
        return Err(syn::Error::new(
            lifetime.span(),
            "tables can contain at most a single lifetime",
        ));
    }

    let result = generics.lifetimes().next().map(|lt| &lt.lifetime).cloned();
    Ok(result)
}
