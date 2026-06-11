//! codegen for table objects

use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::spanned::Spanned;

use crate::{
    parsing::{logged_syn_error, Attr, Field, Table, TableReadArg, TableReadArgs},
    Phase,
};

pub(crate) fn generate(item: &Table) -> syn::Result<TokenStream> {
    if item.attrs.write_only.is_some() {
        return Ok(Default::default());
    }
    let docs = &item.attrs.docs;
    let generic = item.attrs.generic_offset.as_ref();
    let generic_with_default = generic.map(|t| quote!(#t = ()));
    let phantom_decl = generic.map(|t| quote!(offset_type: std::marker::PhantomData<*const #t>));
    let raw_name = item.raw_name();
    let field_byte_range_fns = item.iter_field_byte_range_fns();
    let optional_min_byte_range_trait_impl = item.impl_min_byte_range_trait();
    let min_valid_size = item.min_valid_size_expr();
    let stored_args = item
        .attrs
        .read_args
        .as_ref()
        .map(|args| args.constructor_args())
        .into_iter()
        .flatten();

    let of_unit_docs = " Replace the specific generic type on this implementation with `()`";

    let impl_of_unit_type = generic.as_ref().map(|t| {
        quote! {
               impl<'a, #t> #raw_name<'a, #t> {
                   #[allow(dead_code)]
                   #[doc = #of_unit_docs]
                   pub(crate) fn of_unit_type(&self) -> #raw_name<'a, ()> {
                       #raw_name {
                           data: self.data,
                           offset_type: std::marker::PhantomData,
                       }
                   }
               }
        }
    });

    let table_ref_getters = item.iter_table_ref_getters();
    let optional_format_trait_impl = item.impl_format_trait();
    let optional_discriminant_trait_impl = item.impl_discriminant_trait();
    let font_read = generate_font_read(item)?;
    let debug = generate_debug(item)?;
    let top_level = item.attrs.tag.as_ref().map(|tag| {
        let tag_str = tag.value();
        let doc = format!(" `{tag_str}`");
        let byte_tag = syn::LitByteStr::new(tag_str.as_bytes(), tag.span());
        quote! {
            impl TopLevelTable for #raw_name<'_> {
                #[doc = #doc]
                const TAG: Tag = Tag::new(#byte_tag);
            }
        }
    });

    let const_generic = generic.is_some().then(|| quote!(::<()>));
    // Generate Default for all tables, skipping tables that have a 'format' field
    // that is not expected to be 0 or 1.
    let table_format = item.format_value_and_width();

    // Choose the data constructor based on the table's format value: format=1 needs the
    // first byte set to 1 so that reading back the format field yields the correct value.
    let data_method = match table_format {
        Some((1, 1)) => Some(quote!(default_format_1_u8_table_data)),
        Some((1, 2)) => Some(quote!(default_format_1_u16_table_data)),
        None | Some((0, _)) => Some(quote!(default_table_data)),
        _ => None,
    };

    let impl_default = data_method.map(|data_method| {
        let phantom_init = generic.map(|_| quote!(offset_type: std::marker::PhantomData,));
        let default_args = item
            .attrs
            .read_args
            .as_ref()
            .map(|args| args.idents().map(|id| quote!(#id: Default::default(),)).collect::<Vec<_>>())
            .unwrap_or_default();

        let min_size_is_zero = min_valid_size.to_string() == "0";
        // selectively allow this lint so we can assert 0 <= NULL_POOL_SIZE,
        // which I prefer to elliding the assert
        let size_check_allow =
            min_size_is_zero.then(|| quote!(#[allow(clippy::absurd_extreme_comparisons)]));

        // For generic tables (e.g. Lookup<'a, T>), implement Default for all T
        // so that generic groups can construct a default value for any variant.
        let (impl_generics, type_generics) = if let Some(t) = generic {
            (quote!(<#t>), quote!(<'_, #t>))
        } else {
            (quote!(), quote!(<'_>))
        };

        quote! {
            #size_check_allow
            const _: () = assert!(FontData::default_data_long_enough(#raw_name #const_generic ::MIN_SIZE));

            impl #impl_generics Default for #raw_name #type_generics {
                fn default() -> Self {
                    Self {
                        data: FontData::#data_method(),
                        #( #default_args )*
                        #phantom_init
                    }
                }
            }
        }
    });

    Ok(quote! {
        #optional_format_trait_impl

        #optional_discriminant_trait_impl

        #optional_min_byte_range_trait_impl

        #top_level

        #font_read

        #impl_of_unit_type

        #( #docs )*
        #[derive(Clone)]
        pub struct #raw_name<'a, #generic_with_default> {
            data: FontData<'a>,
            #( #stored_args, )*
            #phantom_decl
        }

        #[allow(clippy::needless_lifetimes)]
        impl<'a, #generic> #raw_name<'a, #generic> {
            pub const MIN_SIZE: usize = #min_valid_size;
            basic_table_impls!(impl_the_methods);

            #( #table_ref_getters )*
            #( #field_byte_range_fns )*

        }

        #impl_default

        #debug
    })
}

fn generate_font_read(item: &Table) -> syn::Result<TokenStream> {
    let name = item.raw_name();
    let generic = item.attrs.generic_offset.as_ref();
    let phantom = generic.map(|_| quote!(offset_type: std::marker::PhantomData,));

    let read_args = match item.attrs.read_args.as_ref() {
        Some(args) => args.attr.clone(),
        None => Default::default(),
    };
    let error_if_phantom_and_read_args =
        (generic.is_some() && !read_args.args.is_empty()).then(|| {
            quote! { compile_error!("ReadWithArgs not implemented for tables with phantom params."); }
        });
    let args_type = read_args.args_type();
    let destructure_pattern = read_args.destructure_pattern();
    let constructor_args = read_args.constructor_args();
    let args_from_constructor_args = read_args.read_args_from_constructor_args();
    let arg_idents = read_args.idents();
    let args_arg = if read_args.args.is_empty() {
        quote!(_)
    } else {
        quote!(args)
    };

    let maybe_custom_read_fn = (!read_args.args.is_empty()).then(|| quote! {

        impl<'a> #name<'a> {
            /// A constructor that requires additional arguments.
            ///
            /// This type requires some external state in order to be
            /// parsed.
            pub fn read(data: FontData<'a>, #( #constructor_args, )* ) -> Result<Self, ReadError> {
                let args = #args_from_constructor_args;
                Self::read_with_args(data, &args)
            }
        }
    });
    let read_args_impl_header = match generic {
        Some(generic) => quote!(impl<#generic> ReadArgs for #name<'_, #generic>),
        None => quote!(impl ReadArgs for #name<'_>),
    };
    Ok(quote! {
        #error_if_phantom_and_read_args
        #read_args_impl_header {
            type Args = #args_type;
        }

        impl<'a, #generic> FontRead<'a> for #name<'a, #generic> {
            fn read_with_args(data: FontData<'a>, #args_arg: &#args_type) -> Result<Self, ReadError> {
                #destructure_pattern
                #[allow(clippy::absurd_extreme_comparisons)] // if MIN_SIZE is 0
                if data.len() < Self::MIN_SIZE {
                    return Err(ReadError::OutOfBounds);
                }
                 Ok(
                     Self {
                         data,
                         #( #arg_idents, )*
                         #phantom
                     }
                 )
            }
        }
        #maybe_custom_read_fn
    })
}

fn generate_debug(item: &Table) -> syn::Result<TokenStream> {
    let name = item.raw_name();
    let name_str = name.to_string();
    let generic = item.attrs.generic_offset.as_ref();
    let generic_bounds = generic
        .is_some()
        .then(|| quote!(: FontRead<'a, Args = ()> + SomeTable<'a> + 'a));
    let field_arms = item.fields.iter_field_traversal_match_arms(false);
    let attrs = item.fields.fields.is_empty().then(|| {
        quote! {
            #[allow(unused_variables)]
            #[allow(clippy::match_single_binding)]
        }
    });

    Ok(quote! {
        #[cfg(feature = "experimental_traverse")]
        impl<'a, #generic #generic_bounds> SomeTable<'a> for #name <'a, #generic> {
            fn type_name(&self) -> &str {
                #name_str
            }

            #attrs
            fn get_field(&self, idx: usize) -> Option<Field<'a>> {
                match idx {
                    #( #field_arms, )*
                    _ => None,
                }
            }
        }

        #[cfg(feature = "experimental_traverse")]
        #[allow(clippy::needless_lifetimes)]
        impl<'a, #generic #generic_bounds> std::fmt::Debug for #name<'a, #generic> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                (self as &dyn SomeTable<'a>).fmt(f)
            }
        }
    })
}

pub(crate) fn generate_compile(item: &Table, parse_module: &syn::Path) -> syn::Result<TokenStream> {
    let decl = super::record::generate_compile_impl(item.raw_name(), &item.attrs, &item.fields)?;
    if decl.is_empty() {
        return Ok(decl);
    }

    let to_owned_impl = item
        .attrs
        .skip_from_obj
        .is_none()
        .then(|| generate_to_owned_impl(item, parse_module))
        .transpose()?;
    let top_level = item.attrs.tag.as_ref().map(|tag| {
        let name = item.raw_name();
        let byte_tag = syn::LitByteStr::new(tag.value().as_bytes(), tag.span());
        quote! {
            impl TopLevelTable for #name {
                const TAG: Tag = Tag::new(#byte_tag);
            }
        }
    });
    Ok(quote! {
        #decl
        #top_level
        #to_owned_impl
    })
}

fn generate_to_owned_impl(item: &Table, parse_module: &syn::Path) -> syn::Result<TokenStream> {
    let name = item.raw_name();
    let field_to_owned_stmts = item.fields.iter_from_obj_ref_stmts(false);
    let comp_generic = item.attrs.generic_offset.as_ref().map(|attr| &attr.attr);
    let parse_generic = comp_generic
        .is_some()
        .then(|| syn::Ident::new("U", Span::call_site()));
    let impl_generics = comp_generic.into_iter().chain(parse_generic.as_ref());
    let impl_generics2 = impl_generics.clone();
    let where_clause = comp_generic.map(|t| {
        quote! {
            where
                U: FontRead<'a, Args = ()>,
                #t: FromTableRef<U> + Default + 'static,
        }
    });

    let impl_font_read = item.attrs.read_args.is_none() && item.attrs.generic_offset.is_none();
    let maybe_font_read = impl_font_read.then(|| {
        quote! {
            impl ReadArgs for #name {
                type Args = ();
            }

            impl<'a> FontRead<'a> for #name {
                fn read_with_args(data: FontData<'a>, _: &()) -> Result<Self, ReadError> {
                    <#parse_module :: #name as FontRead>::read(data)
                        .map(|x| x.to_owned_table())
                }
            }
        }
    });

    let should_bind_offset_data = item.fields.from_obj_requires_offset_data(false);
    let offset_data_src = item.fields.iter().find_map(|fld| {
        fld.attrs
            .offset_data
            .as_ref()
            .map(|Attr { attr, .. }| quote!(#attr))
    });
    let maybe_bind_offset_data = should_bind_offset_data.then(|| match offset_data_src {
        Some(ident) => quote!(let offset_data = obj. #ident ();),
        None => quote!( let offset_data = obj.offset_data(); ),
    });

    Ok(quote! {
        impl<'a, #( #impl_generics, )* > FromObjRef<#parse_module :: #name<'a, #parse_generic>> for #name<#comp_generic> #where_clause {
            fn from_obj_ref(obj: &#parse_module :: #name<'a, #parse_generic>, _: FontData) -> Self {
                #maybe_bind_offset_data
                #name {
                    #( #field_to_owned_stmts, )*
                }
            }
        }

        #[allow(clippy::needless_lifetimes)]
        impl<'a, #(#impl_generics2,)* > FromTableRef<#parse_module :: #name<'a, #parse_generic >> for #name<#comp_generic> #where_clause {}

        #maybe_font_read
    })
}

impl Table {
    pub(crate) fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        for fld in self.fields.iter() {
            if fld.attrs.discriminant.is_some() && self.attrs.generic_offset.is_none() {
                return Err(logged_syn_error(
                    fld.attrs.discriminant.as_ref().unwrap().span(),
                    "#[discriminant] is only valid in tables with #[generic_offset]",
                ));
            }
        }
        self.fields.sanity_check(phase)
    }

    fn iter_field_byte_range_fns(&self) -> impl Iterator<Item = TokenStream> + '_ {
        let mut prev_field_end_expr = quote!(0);
        let mut iter = self.fields.iter();

        std::iter::from_fn(move || {
            let field = iter.next()?;
            let fn_name = field.shape_byte_range_fn_name();
            let len_expr = field.field_len_expr();
            let required_field_decls = field.count_arg_names().map(|fld| {
                let is_opt = self
                    .fields
                    .find(fld)
                    .map(|x| x.is_conditional())
                    .unwrap_or(false);
                let maybe_unwrap_or_default = (is_opt).then(|| quote!(.unwrap_or_default()));
                quote!(let #fld = self.#fld() #maybe_unwrap_or_default ;)
            });

            // okay so for conditions, how do we evaluate them?
            let condition = field
                .attrs
                .conditional
                .as_ref()
                .map(|cond| cond.condition_tokens_for_read());

            let end_expr = if let Some(condition) = condition {
                quote! {
                    if #condition { start + #len_expr } else { start }
                }
            } else {
                quote!( start + #len_expr)
            };
            let result = quote! {
                pub fn #fn_name(&self) -> Range<usize> {
                    #( #required_field_decls )*
                    let start = #prev_field_end_expr;
                    let end = #end_expr;
                    start..end
                }
            };
            prev_field_end_expr = quote!( self.#fn_name().end );

            Some(result)
        })
    }

    fn iter_table_ref_getters(&self) -> impl Iterator<Item = TokenStream> + '_ {
        let generic = self.attrs.generic_offset.as_ref().map(|attr| &attr.attr);
        self.fields
            .iter()
            .filter_map(move |fld| fld.table_getter(generic))
            .chain(
                self.attrs
                    .read_args
                    .as_ref()
                    .into_iter()
                    .flat_map(|args| args.iter_table_ref_getters()),
            )
    }

    pub(crate) fn impl_format_trait(&self) -> Option<TokenStream> {
        let field = self.fields.iter().find(|fld| fld.attrs.format.is_some())?;
        let name = self.raw_name();
        let value = &field.attrs.format.as_ref().unwrap();
        let typ = field.typ.cooked_type_tokens();

        Some(quote! {
            impl Format<#typ> for #name<'_> {
                const FORMAT: #typ = #value;
            }
        })
    }

    pub(crate) fn impl_min_byte_range_trait(&self) -> Option<TokenStream> {
        let field = self
            .fields
            .iter()
            .filter(|fld| fld.attrs.conditional.is_none())
            .last()?;
        let name = self.raw_name();
        let generic = self.attrs.generic_offset.as_ref();

        let fn_name = field.shape_byte_range_fn_name();
        Some(quote! {
            impl<'a, #generic> MinByteRange<'a> for #name<'a, #generic> {
                fn min_byte_range(&self) -> Range<usize> {
                    0..self.#fn_name().end
                }
                fn min_table_bytes(&self) -> &'a [u8] {
                    let range = self.min_byte_range();
                    self.data.as_bytes().get(range).unwrap_or_default()
                }
            }
        })
    }

    pub(crate) fn impl_discriminant_trait(&self) -> Option<TokenStream> {
        let field = self
            .fields
            .iter()
            .find(|fld| fld.attrs.discriminant.is_some())?;
        let name = self.raw_name();

        // Compute the static byte offset of the discriminant field by summing
        // the known sizes of all preceding fields.
        let offset_parts: Vec<_> = self
            .fields
            .iter()
            .take_while(|fld| fld.name != field.name)
            .map(|fld| {
                fld.known_min_size_stmt()
                    .expect("all fields before #[discriminant] must have a known size")
            })
            .filter(|tokens| !tokens.is_empty())
            .collect();
        let offset_expr = match offset_parts.as_slice() {
            [] => quote!(0),
            [one] => one.to_owned(),
            more => quote!( (#(#more)+*) ),
        };

        Some(quote! {
            impl Discriminant for #name<'_, ()> {
                fn read_discriminant(data: FontData<'_>) -> Result<u16, ReadError> {
                    data.read_at(#offset_expr)
                }
            }
        })
    }

    pub(crate) fn min_valid_size_expr(&self) -> TokenStream {
        let field_sizes = self
            .fields
            .iter()
            .map_while(Field::known_min_size_stmt)
            .filter(|tokens| !tokens.is_empty())
            .collect::<Vec<_>>();
        match field_sizes.as_slice() {
            [] => quote!(0),
            [one] => one.to_owned(),
            more => quote!( (#(#more)+*) ),
        }
    }
}

impl TableReadArgs {
    pub(crate) fn args_type(&self) -> TokenStream {
        match self.args.as_slice() {
            [TableReadArg { typ, .. }] => typ.to_token_stream(),
            other => {
                let typs = other.iter().map(|arg| &arg.typ);
                quote!( ( #(#typs,)* ) )
            }
        }
    }

    pub(crate) fn destructure_pattern(&self) -> TokenStream {
        match self.args.as_slice() {
            [] => Default::default(),
            [TableReadArg { ident, .. }] => quote!(let #ident = *args;),
            other => {
                let idents = other.iter().map(|arg| &arg.ident);
                quote!( let  ( #(#idents,)* ) = *args; )
            }
        }
    }

    pub(crate) fn constructor_args(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.args
            .iter()
            .map(|TableReadArg { ident, typ }| quote!(#ident: #typ))
    }

    pub(crate) fn idents(&self) -> impl Iterator<Item = &syn::Ident> + '_ {
        self.args.iter().map(|arg| &arg.ident)
    }

    // if only one arg then just that, else a tuple of args
    pub(crate) fn read_args_from_constructor_args(&self) -> TokenStream {
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
                    self.#ident
                }
            }
        })
    }
}
