//! codegen for table objects

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use super::parsing::{Field, Table, TableFormat, TableReadArg, TableReadArgs};

pub(crate) fn generate(item: &Table) -> syn::Result<TokenStream> {
    if item.attrs.skip_parse.is_some() {
        return Ok(Default::default());
    }
    let docs = &item.attrs.docs;
    let marker_name = item.marker_name();
    let raw_name = item.raw_name();
    let shape_byte_range_fns = item.iter_shape_byte_fns();
    let shape_fields = item.iter_shape_fields();

    let table_ref_getters = item.iter_table_ref_getters();

    let optional_format_trait_impl = item.impl_format_trait();
    let font_read = generate_font_read(item)?;

    Ok(quote! {
        #optional_format_trait_impl

        #( #docs )*
        #[derive(Debug, Clone, Copy)]
        #[doc(hidden)]
        pub struct #marker_name {
            #( #shape_fields ),*
        }

        impl #marker_name {
            #( #shape_byte_range_fns )*
        }

        #font_read

        #( #docs )*
        pub type #raw_name<'a> = TableRef<'a, #marker_name>;

        impl<'a> #raw_name<'a> {

            #( #table_ref_getters )*

        }
    })
}

fn generate_font_read(item: &Table) -> syn::Result<TokenStream> {
    let marker_name = item.marker_name();
    let field_validation_stmts = item.iter_field_validation_stmts();
    let shape_field_names = item.iter_shape_field_names();

    // add this attribute if we're going to be generating expressions which
    // may trigger a warning
    let ignore_parens = item
        .fields
        .iter()
        .any(|fld| fld.has_computed_len())
        .then(|| quote!(#[allow(unused_parens)]));

    if let Some(read_args) = &item.attrs.read_args {
        let args_type = read_args.args_type();
        let destructure_pattern = read_args.destructure_pattern();
        Ok(quote! {
            impl ReadArgs for #marker_name {
                type Args = #args_type;
            }

            impl TableInfoWithArgs for #marker_name {
                #ignore_parens
                fn parse_with_args<'a>(data: FontData<'a>, args: &#args_type) -> Result<TableRef<'a, Self>, ReadError> {
                    let #destructure_pattern = *args;
                    let mut cursor = data.cursor();
                    #( #field_validation_stmts )*
                    cursor.finish( #marker_name {
                        #( #shape_field_names, )*
                    })
                }
            }
        })
    } else {
        Ok(quote! {
            impl TableInfo for #marker_name {
            #ignore_parens
            fn parse<'a>(data: FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
                let mut cursor = data.cursor();
                #( #field_validation_stmts )*
                cursor.finish( #marker_name {
                    #( #shape_field_names, )*
                })
            }
        }
        })
    }
}

pub(crate) fn generate_compile(item: &Table) -> syn::Result<TokenStream> {
    super::record::generate_compile_impl(item.raw_name(), &item.attrs, &item.fields)
}

pub(crate) fn generate_format_compile(item: &TableFormat) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.docs;
    let variants = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = variant.type_name();
        let docs = &variant.docs;
        quote! ( #( #docs )* #name(#typ) )
    });

    let write_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.write_into(writer), )
    });

    let validation_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.validate_impl(ctx), )
    });

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug)]
        pub enum #name {
            #( #variants ),*
        }

        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                match self {
                    #( #write_arms )*
                }
            }
        }

        impl Validate for #name {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                match self {
                    #( #validation_arms )*
                }
            }
        }
    })
}

pub(crate) fn generate_format_group(item: &TableFormat) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.docs;
    let variants = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = variant.type_name();
        let docs = &variant.docs;
        quote! ( #( #docs )* #name(#typ<'a>) )
    });

    let format = &item.format;
    let match_arms = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = variant.marker_name();
        quote! {
            #typ::FORMAT => {
                Ok(Self::#name(FontRead::read(data)?))
            }
        }
    });

    Ok(quote! {
        #( #docs )*
        pub enum #name<'a> {
            #( #variants ),*
        }

        impl<'a> FontRead<'a> for #name<'a> {
            fn read(data: FontData<'a>) -> Result<Self, ReadError> {
                let format: #format = data.read_at(0)?;
                match format {
                    #( #match_arms ),*
                    other => Err(ReadError::InvalidFormat(other)),
                }
            }
        }
    })
}

impl Table {
    fn marker_name(&self) -> syn::Ident {
        quote::format_ident!("{}Marker", self.raw_name())
    }

    fn iter_shape_byte_fns(&self) -> impl Iterator<Item = TokenStream> + '_ {
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

    fn iter_shape_fields(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.iter_shape_field_names_and_types()
            .into_iter()
            .map(|(ident, typ)| quote!( #ident: #typ ))
    }

    fn iter_shape_field_names(&self) -> impl Iterator<Item = syn::Ident> + '_ {
        self.iter_shape_field_names_and_types()
            .into_iter()
            .map(|(name, _)| name)
    }

    fn iter_shape_field_names_and_types(&self) -> Vec<(syn::Ident, TokenStream)> {
        let mut result = Vec::new();
        // we always save input args in the shape. We could be more judicious,
        // but this is an uncommon case
        if let Some(args) = &self.attrs.read_args {
            result.extend(
                args.args
                    .iter()
                    .map(|arg| (arg.ident.clone(), arg.typ.to_token_stream())),
            );
        }

        for next in self.fields.iter() {
            let is_versioned = next.attrs.available.is_some();
            let has_computed_len = next.has_computed_len();
            if !(is_versioned || has_computed_len) {
                continue;
            }
            if is_versioned {
                let field_name = next.shape_byte_start_field_name();
                result.push((field_name, quote!(Option<usize>)));
            }

            if has_computed_len {
                let field_name = next.shape_byte_len_field_name();
                if is_versioned {
                    result.push((field_name, quote!(Option<usize>)));
                } else {
                    result.push((field_name, quote!(usize)));
                }
            };
        }
        result
    }

    fn iter_field_validation_stmts(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().map(Field::field_parse_validation_stmts)
    }

    fn iter_table_ref_getters(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().filter_map(Field::table_getter).chain(
            self.attrs
                .read_args
                .as_ref()
                .into_iter()
                .flat_map(|args| args.iter_table_ref_getters()),
        )
    }

    pub(crate) fn impl_format_trait(&self) -> Option<TokenStream> {
        let field = self.fields.iter().find(|fld| fld.attrs.format.is_some())?;
        let name = self.marker_name();
        let value = &field.attrs.format.as_ref().unwrap().value;
        let typ = field.typ.cooked_type_tokens();

        Some(quote! {
            impl Format<#typ> for #name {
                const FORMAT: #typ = #value;
            }
        })
    }
}

impl TableReadArgs {
    fn args_type(&self) -> TokenStream {
        match self.args.as_slice() {
            [TableReadArg { typ, .. }] => typ.to_token_stream(),
            other => {
                let typs = other.iter().map(|arg| &arg.typ);
                quote!( ( #(#typs,)* ) )
            }
        }
    }

    fn destructure_pattern(&self) -> TokenStream {
        match self.args.as_slice() {
            [TableReadArg { ident, .. }] => ident.to_token_stream(),
            other => {
                let idents = other.iter().map(|arg| &arg.ident);
                quote!( ( #(#idents,)* ) )
            }
        }
    }

    fn iter_table_ref_getters(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.args.iter().map(|TableReadArg { ident, typ }| {
            quote! {
                pub(crate) fn #ident(&self) -> #typ {
                    self.shape.#ident
                }
            }
        })
    }
}
