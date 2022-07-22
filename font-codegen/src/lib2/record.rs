//! codegen for record objects

use proc_macro2::TokenStream;
use quote::quote;

use super::parsing::{Field, Fields, Record, TableAttrs};

pub(crate) fn generate(item: &Record) -> syn::Result<proc_macro2::TokenStream> {
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
    let inner_types = item.fields.iter().map(|fld| fld.raw_getter_return_type());

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug)]
        #[repr(C)]
        #[repr(packed)]
        pub struct #name {
            #( #field_docs pub #field_names: #field_types, )*
        }

        impl FixedSized for #name {
            const RAW_BYTE_LEN: usize = #( #inner_types::RAW_BYTE_LEN )+*;
        }
    })
}

pub(crate) fn generate_compile(item: &Record) -> syn::Result<proc_macro2::TokenStream> {
    generate_compile_impl(&item.name, &item.attrs, &item.fields)
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
