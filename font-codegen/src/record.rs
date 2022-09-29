//! codegen for record objects

use proc_macro2::TokenStream;
use quote::quote;

use super::parsing::{CustomCompile, Field, Fields, Record, TableAttrs};

pub(crate) fn generate(item: &Record) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let field_names = item.fields.iter().map(|fld| &fld.name).collect::<Vec<_>>();
    let field_types = item
        .fields
        .iter()
        .map(Field::type_for_record)
        .collect::<Vec<_>>();
    let field_docs = item.fields.iter().map(|fld| {
        let docs = &fld.attrs.docs;
        quote!( #( #docs )* )
    });
    let getters = item.fields.iter().map(|fld| fld.record_getter(item));
    let extra_traits = generate_extra_traits(item)?;
    let traversal_impl = generate_traversal(item)?;

    let repr_packed = item.lifetime.is_none().then(|| {
        quote! {
            #[repr(C)]
            #[repr(packed)]
        }
    });

    let lifetime = &item.lifetime;

    Ok(quote! {
    #( #docs )*
    #[derive(Clone, Debug)]
    #repr_packed
    pub struct #name #lifetime {
        #( #field_docs pub #field_names: #field_types, )*
    }

    impl #lifetime #name #lifetime {
        #( #getters )*
    }

    #extra_traits
    #traversal_impl
        })
}

fn generate_extra_traits(item: &Record) -> syn::Result<TokenStream> {
    let name = &item.name;
    let lifetime = &item.lifetime;
    let anon_lifetime = lifetime.is_some().then(|| quote!(<'_>));

    if item.attrs.read_args.is_none() {
        let inner_types = item.fields.iter().map(|fld| fld.typ.cooked_type_tokens());
        return Ok(quote! {
            impl FixedSized for #name {
                const RAW_BYTE_LEN: usize = #( #inner_types::RAW_BYTE_LEN )+*;
            }
        });
    }

    let args = item.attrs.read_args.as_ref().unwrap();
    let args_type = args.args_type();
    let destructure_pattern = args.destructure_pattern();
    let field_size_expr = item.fields.iter().map(Field::record_len_expr);
    let field_inits = item.fields.iter().map(Field::record_init_stmt);

    Ok(quote! {
        impl ReadArgs for #name #anon_lifetime {
            type Args = #args_type;
        }

        impl ComputeSize for #name #anon_lifetime {
            fn compute_size(args: &#args_type) -> usize {
                let #destructure_pattern = *args;
                #( #field_size_expr )+*
            }
        }

        impl<'a> FontReadWithArgs<'a> for #name #lifetime {
            #[allow(unused_parens)]
            fn read_with_args(data: FontData<'a>, args: &#args_type) -> Result<Self, ReadError> {
                let mut cursor = data.cursor();
                let #destructure_pattern = *args;
                Ok(Self {
                    #( #field_inits, )*
                })

            }
        }
    })
}

fn generate_traversal(item: &Record) -> syn::Result<TokenStream> {
    let name = &item.name;
    let name_str = name.to_string();
    let lifetime = &item.lifetime;
    let field_arms = item.fields.iter_field_traversal_match_arms(true);

    Ok(quote! {
        #[cfg(feature = "traversal")]
        impl<'a> SomeRecord<'a> for #name #lifetime {
            fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
                RecordResolver {
                    name: #name_str,
                    get_field: Box::new(move |idx, _data| match idx {
                        #( #field_arms, )*
                        _ => None,
                    }),
                    data,
                }
            }
        }
    })
}

pub(crate) fn generate_compile(
    item: &Record,
    parse_module: &syn::Path,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut decl = generate_compile_impl(&item.name, &item.attrs, &item.fields)?;
    let to_owned = item
        .attrs
        .skip_from_obj
        .is_none()
        .then(|| generate_from_obj_impl(item, parse_module))
        .transpose()?;
    decl.extend(to_owned);
    Ok(decl)
}

// shared between table/record
pub(crate) fn generate_compile_impl(
    name: &syn::Ident,
    attrs: &TableAttrs,
    fields: &Fields,
) -> syn::Result<TokenStream> {
    let docs = &attrs.docs;
    let field_decls = fields.iter_compile_decls();
    let generic_param = attrs.phantom.as_ref();
    let maybe_allow_casts = fields
        .compile_write_contains_int_casts()
        .then(|| quote!(#[allow(clippy::unnecessary_cast)]));

    // if we have fields that should be present for a specific version, declare
    // a 'version' binding at the top of our validation block
    let needs_version_decl = fields
        .iter()
        .any(|fld| fld.attrs.available.is_some() && fld.attrs.nullable.is_none());

    let version_decl = fields
        .version_field()
        .filter(|_| needs_version_decl)
        .map(|fld| {
            let name = &fld.name;
            match fld.attrs.compile.as_ref().map(|attr| &attr.attr) {
                Some(CustomCompile::Expr(inline_expr)) => {
                    let typ = fld.typ.cooked_type_tokens();
                    let expr = inline_expr.compile_expr();
                    quote! { let version: #typ = #expr; }
                }
                Some(_) => panic!("version fields are never skipped"),
                None => quote! { let version = self.#name; },
            }
        });

    let write_stmts = fields.iter_compile_write_stmts();
    let write_impl_params = generic_param.map(|t| quote! { <#t: FontWrite> });
    let validate_impl_params = generic_param.map(|t| quote! { <#t: Validate> });

    let name_string = name.to_string();
    let custom_validation = attrs.validation_method.as_ref().map(|path| {
        quote! (
                self.#path(ctx);
        )
    });
    let validation_stmts = fields.compilation_validation_stmts();
    let validation_fn = if custom_validation.is_none() && validation_stmts.is_empty() {
        quote!(
            fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
        )
    } else {
        quote! {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                ctx.in_table(#name_string, |ctx| {
                    #custom_validation
                    #version_decl
                    #( #validation_stmts)*
                })
            }
        }
    };

    let validation_impl = quote! {
        impl #validate_impl_params Validate for #name <#generic_param> {
            #validation_fn
        }
    };

    let font_write_impl = attrs.skip_font_write.is_none().then(|| {
        quote! {
            impl #write_impl_params FontWrite for #name <#generic_param> {
                #maybe_allow_casts
                fn write_into(&self, writer: &mut TableWriter) {
                    #( #write_stmts; )*
                }
            }
        }
    });

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug)]
        pub struct #name <#generic_param> {
            #( #field_decls, )*
        }

        #font_write_impl

        #validation_impl

    })
}

fn generate_from_obj_impl(item: &Record, parse_module: &syn::Path) -> syn::Result<TokenStream> {
    let name = &item.name;
    let lifetime = item.lifetime.is_some().then(|| quote!(<'_>));
    let field_to_owned_stmts = item.fields.iter_from_obj_ref_stmts(true);
    let offset_data_ident = if item.fields.from_obj_requires_offset_data(true) {
        quote!(offset_data)
    } else {
        quote!(_)
    };

    Ok(quote! {
        #[cfg(feature = "parsing")]
        impl FromObjRef<#parse_module:: #name #lifetime> for #name {
            fn from_obj_ref(obj: &#parse_module:: #name, #offset_data_ident: FontData) -> Self {
                #name {
                    #( #field_to_owned_stmts, )*
                }
            }
        }
    })
}

impl Record {
    pub(crate) fn sanity_check(&self) -> syn::Result<()> {
        self.fields.sanity_check()?;
        let field_needs_lifetime = self
            .fields
            .iter()
            .find(|fld| fld.is_computed_array() || fld.is_array());
        match (field_needs_lifetime, &self.lifetime) {
            (Some(_), None) => Err(syn::Error::new(
                self.name.span(),
                "This record contains an array, and so must have a lifetime",
            )),
            (None, Some(_)) => Err(syn::Error::new(
                self.name.span(),
                "unexpected lifetime; record contains no array",
            )),
            _ => Ok(()),
        }
    }
}
