//! codegen for record objects

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use crate::{
    fields::FieldConstructorInfo,
    parsing::{
        logged_syn_error, CustomCompile, Field, FieldType, Fields, Item, Items, Phase, Record,
        TableAttrs,
    },
};

pub(crate) fn generate(item: &Record, all_items: &Items) -> syn::Result<TokenStream> {
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
    let traversal_impl = generate_traversal(item)?;

    let lifetime = &item.lifetime;
    let is_zerocopy = item.is_zerocopy();
    let has_read_args = item.attrs.read_args.is_some();
    let repr_packed = is_zerocopy.then(|| {
        quote! {
            #[repr(C)]
            #[repr(packed)]
        }
    });
    let simple_record_traits = is_zerocopy.then(|| quote!(Copy, bytemuck::AnyBitPattern,));

    let maybe_impl_fixed_size = is_zerocopy.then(|| {
        let inner_types = item.fields.iter().map(|fld| fld.typ.cooked_type_tokens());
        quote! {
            impl FixedSize for #name {
                const RAW_BYTE_LEN: usize = #( #inner_types::RAW_BYTE_LEN )+*;
            }

        }
    });
    let maybe_impl_read_with_args = (has_read_args).then(|| generate_read_with_args(item));
    let maybe_extra_traits = item
        .gets_extra_traits(all_items)
        .then(|| quote!(PartialEq, Eq, PartialOrd, Ord, Hash,));
    Ok(quote! {
    #( #docs )*
    #[derive(Clone, Debug, #maybe_extra_traits #simple_record_traits )]
    #repr_packed
    pub struct #name #lifetime {
        #( #field_docs pub #field_names: #field_types, )*
    }

    impl #lifetime #name #lifetime {
        #( #getters )*
    }

    #maybe_impl_fixed_size
    #maybe_impl_read_with_args
    #traversal_impl
        })
}

fn generate_read_with_args(item: &Record) -> TokenStream {
    assert!(item.attrs.read_args.is_some()); // expected this to be checked already
                                             //
    let name = &item.name;
    let lifetime = &item.lifetime;
    let anon_lifetime = lifetime.is_some().then(|| quote!(<'_>));

    let args = item.attrs.read_args.as_ref().unwrap();
    let args_type = args.args_type();
    let destructure_pattern = args.destructure_pattern();
    let field_size_expr: Vec<_> = item.fields.iter().map(Field::record_len_expr).collect();
    let field_inits = item.fields.iter().map(Field::record_init_stmt);
    let constructor_args = args.constructor_args();
    let args_from_constructor_args = args.read_args_from_constructor_args();

    let comp_size_expr = match field_size_expr.as_slice() {
        [] => panic!("should never be empty"),
        [one_expr] => quote!( Ok(#one_expr) ),
        exprs => {
            quote! {
                let mut result = 0usize;
                #( result = result.checked_add(#exprs).ok_or(ReadError::OutOfBounds)?; )*
                Ok(result)
            }
        }
    };

    quote! {
        impl ReadArgs for #name #anon_lifetime {
            type Args = #args_type;
        }

        impl ComputeSize for #name #anon_lifetime {
            #[allow(clippy::needless_question_mark)]
            fn compute_size(args: &#args_type) -> Result<usize, ReadError> {
                let #destructure_pattern = *args;
                #comp_size_expr
            }
        }

        impl<'a> FontReadWithArgs<'a> for #name #lifetime {
            fn read_with_args(data: FontData<'a>, args: &#args_type) -> Result<Self, ReadError> {
                let mut cursor = data.cursor();
                let #destructure_pattern = *args;
                Ok(Self {
                    #( #field_inits, )*
                })

            }
        }

        #[allow(clippy::needless_lifetimes)]
        impl<'a> #name #lifetime {
            /// A constructor that requires additional arguments.
            ///
            /// This type requires some external state in order to be
            /// parsed.
            pub fn read(data: FontData<'a>, #( #constructor_args, )* ) -> Result<Self, ReadError> {
                let args = #args_from_constructor_args;
                Self::read_with_args(data, &args)
            }
        }
    }
}

fn generate_traversal(item: &Record) -> syn::Result<TokenStream> {
    let name = &item.name;
    let name_str = name.to_string();
    let lifetime = &item.lifetime;
    let field_arms = item.fields.iter_field_traversal_match_arms(true);

    Ok(quote! {
        #[cfg(feature = "experimental_traverse")]
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
    let generic_param = attrs.generic_offset.as_ref();
    let maybe_allow_casts = fields
        .compile_write_contains_int_casts()
        .then(|| quote!(#[allow(clippy::unnecessary_cast)]));

    // if we have fields that should be present for a specific version, declare
    // a 'version' binding at the top of our validation block
    let needs_version_decl = fields
        .iter()
        .any(|fld| fld.attrs.conditional.is_some() && fld.attrs.nullable.is_none());

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

    let conditional_inputs = fields
        .conditional_input_idents()
        .into_iter()
        .map(|fld| quote!(let #fld = self.#fld;));

    let write_stmts = fields.iter_compile_write_stmts();
    let write_impl_params = generic_param.map(|t| quote! { <#t: FontWrite> });
    let validate_impl_params = generic_param.map(|t| quote! { <#t: Validate> });

    let name_string = name.to_string();
    let mut validation_stmts = fields.compilation_validation_stmts();
    if let Some(validation_ident) = attrs.validate.as_ref() {
        validation_stmts.push(quote!(
            self.#validation_ident(ctx);
        ));
    }
    let validation_fn = if validation_stmts.is_empty() {
        quote!(
            fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
        )
    } else {
        quote! {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                ctx.in_table(#name_string, |ctx| {
                    #version_decl
                    #( #conditional_inputs )*
                    #( #validation_stmts)*
                })
            }
        }
    };

    let table_type = if attrs.tag.is_some() {
        quote!(TableType::TopLevel( #name::TAG ))
    } else {
        quote!( TableType::Named( #name_string ) )
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

                fn table_type(&self) -> TableType {
                    #table_type
                }
            }
        }
    });

    let can_derive_default = fields.can_derive_default()?;
    let maybe_derive_default = can_derive_default.then(|| quote!(Default,));
    let default_impl_params = generic_param.map(|t| quote! { <#t: Default> });
    let maybe_custom_default = (!can_derive_default).then(|| {
        let default_field_inits = fields.iter_compile_default_inits();
        quote! {
        impl #default_impl_params Default for #name <#generic_param> {
            fn default() -> Self {
                Self {
                    #( #default_field_inits, )*
                }
            }
        }
        }
    });

    let constructor_args_raw = fields.iter_constructor_info().collect::<Vec<_>>();
    let constructor_args = constructor_args_raw.iter().map(
        |FieldConstructorInfo {
             name, arg_tokens, ..
         }| quote!(#name: #arg_tokens),
    );
    let constructor_field_inits = constructor_args_raw.iter().map(
        |FieldConstructorInfo {
             name,
             is_offset,
             is_array,
             ..
         }| {
            if *is_array {
                quote!(#name: #name.into_iter().map(Into::into).collect())
            } else if *is_offset {
                quote!( #name: #name.into())
            } else {
                name.into_token_stream()
            }
        },
    );

    let maybe_constructor = attrs.skip_constructor.is_none().then(|| {
        let docstring = format!(" Construct a new `{name}`");
        let add_defaults = fields
            .iter()
            .any(Field::skipped_in_constructor)
            .then(|| quote!(..Default::default()));
        // judiciously allow this lint
        let too_many_args =
            (constructor_args_raw.len() > 7).then(|| quote!(#[allow(clippy::too_many_arguments)]));
        // if this has a manual compile type we don't know much about it, and
        // will often trigger this lint:
        let useless_conversion = (constructor_args_raw
            .iter()
            .any(|info| info.manual_compile_type))
        .then(|| quote!( #[allow(clippy::useless_conversion)] ));
        quote! {
            impl #default_impl_params #name <#generic_param> {
                #[doc = #docstring]
                #too_many_args
                #useless_conversion
                pub fn new( #( #constructor_args,)*  ) -> Self {
                    Self {
                        #( #constructor_field_inits, )*
                        #add_defaults
                    }
                }
            }
        }
    });

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug, #maybe_derive_default PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct #name <#generic_param> {
            #( #field_decls, )*
        }

        #maybe_custom_default

        #maybe_constructor

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
    pub(crate) fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        self.fields.sanity_check(phase)?;
        let field_needs_lifetime = self
            .fields
            .iter()
            .find(|fld| fld.is_computed_array() || fld.is_array());
        match (field_needs_lifetime, &self.lifetime) {
            (Some(_), None) => Err(logged_syn_error(
                self.name.span(),
                "This record contains an array, and so must have a lifetime",
            )),
            (None, Some(_)) => Err(logged_syn_error(
                self.name.span(),
                "unexpected lifetime; record contains no array",
            )),
            _ => Ok(()),
        }
    }

    fn is_zerocopy(&self) -> bool {
        self.fields.iter().all(Field::is_zerocopy_compatible)
    }

    fn gets_extra_traits(&self, all_items: &Items) -> bool {
        self.fields
            .iter()
            .all(|fld| can_derive_extra_traits(&fld.typ, all_items))
    }
}

/// Returns `true` if this field is composed only of non-offset scalars.
///
/// This means it can contain scalars, records which only contain scalars,
/// and arrays of these two types.
///
/// we do not generate these traits if a record contains an offset,
/// because the semantics are unclear: we would be comparing the raw bytes
/// in the offset, instead of the thing that the offset points to.
fn can_derive_extra_traits(field_type: &FieldType, all_items: &Items) -> bool {
    match field_type {
        FieldType::Scalar { .. } => true,
        FieldType::Struct { typ } => match all_items.get(typ) {
            Some(Item::Record(record)) => record.gets_extra_traits(all_items),
            _ => false,
        },
        FieldType::Array { inner_typ } => can_derive_extra_traits(inner_typ, all_items),
        _ => false,
    }
}
