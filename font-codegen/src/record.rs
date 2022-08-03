//! codegen for record objects

use proc_macro2::TokenStream;
use quote::quote;

use super::parsing::{Field, Fields, Record, TableAttrs};

pub(crate) fn generate(item: &Record) -> syn::Result<TokenStream> {
    if item.attrs.skip_parse.is_some() {
        return Ok(Default::default());
    }

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
        })
}

fn generate_extra_traits(item: &Record) -> syn::Result<TokenStream> {
    let name = &item.name;
    let lifetime = &item.lifetime;
    let anon_lifetime = lifetime.is_some().then(|| quote!(<'_>));

    if item.attrs.read_args.is_none() {
        let inner_types = item.fields.iter().map(|fld| fld.raw_getter_return_type());
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

pub(crate) fn generate_compile(
    item: &Record,
    parse_module: &syn::Path,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut decl = generate_compile_impl(&item.name, &item.attrs, &item.fields)?;
    let to_owned = generate_from_obj_impl(item, parse_module)?;
    decl.extend(to_owned);
    Ok(decl)
}

// shared between table/record
pub(crate) fn generate_compile_impl(
    name: &syn::Ident,
    attrs: &TableAttrs,
    fields: &Fields,
) -> syn::Result<TokenStream> {
    if attrs.skip_compile.is_some() {
        return Ok(Default::default());
    }

    let docs = &attrs.docs;
    let field_decls = fields.iter_compile_decls();
    let write_stmts = fields.iter_compile_write_stmts();

    let name_string = name.to_string();
    let custom_validation = attrs.validation_method.as_ref().map(|path| {
        quote! (
                self.#path(ctx);
        )
    });
    let validation_stmts = fields.compilation_validation_stmts();
    let validation_impl = if custom_validation.is_none() && validation_stmts.is_empty() {
        quote!(
            fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
        )
    } else {
        quote! {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                ctx.in_table(#name_string, |ctx| {
                    #custom_validation
                    #( #validation_stmts)*
                })
            }
        }
    };

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug)]
        pub struct #name {
            #( #field_decls, )*
        }

        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                #( #write_stmts; )*
            }
        }

        impl Validate for #name {
            #validation_impl
        }
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
            fn from_obj_ref(obj: &#parse_module:: #name, #offset_data_ident: &FontData) -> Self {
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
