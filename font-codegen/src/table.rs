//! codegen for table objects

use std::collections::HashMap;

use crate::{fields::FieldConstructorInfo, parsing::logged_syn_error};
use indexmap::IndexMap;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};

use crate::parsing::{Attr, GenericGroup, Item, Items, Phase};

use super::parsing::{Field, ReferencedFields, Table, TableFormat, TableReadArg, TableReadArgs};

pub(crate) fn generate(item: &Table) -> syn::Result<TokenStream> {
    if item.attrs.write_only.is_some() {
        return Ok(Default::default());
    }
    let docs = &item.attrs.docs;
    let generic = item.attrs.generic_offset.as_ref();
    let generic_with_default = generic.map(|t| quote!(#t = ()));
    let phantom_decl = generic.map(|t| quote!(offset_type: std::marker::PhantomData<*const #t>));
    let marker_name = item.marker_name();
    let raw_name = item.raw_name();
    let shape_byte_range_fns = item.iter_shape_byte_fns();
    let optional_min_byte_range_trait_impl = item.impl_min_byte_range_trait();
    let shape_fields = item.iter_shape_fields();
    let derive_clone_copy = generic.is_none().then(|| quote!(Clone, Copy));
    let impl_clone_copy = generic.is_some().then(|| {
        quote! {
            impl<#generic> Clone for #marker_name<#generic> {
                fn clone(&self) -> Self {
                    *self
                }
            }

            impl<#generic> Copy for #marker_name<#generic> {}
        }
    });

    let of_unit_docs = " Replace the specific generic type on this implementation with `()`";

    // In the presence of a generic param we only impl FontRead for Name<()>,
    // and then use into() to convert it to the concrete generic type.
    let impl_into_generic = generic.as_ref().map(|t| {
        let shape_fields = item
            .iter_shape_field_names()
            .map(|name| quote!(#name: shape.#name))
            .collect::<Vec<_>>();

        let shape_name = if shape_fields.is_empty() {
            quote!(..)
        } else {
            quote!(shape)
        };

        quote! {
               impl<'a> #raw_name<'a, ()> {
                   #[allow(dead_code)]
                   pub(crate) fn into_concrete<T>(self) -> #raw_name<'a, #t> {
                       let TableRef { data, #shape_name} = self;
                       TableRef {
                           shape: #marker_name {
                               #( #shape_fields, )*
                               offset_type: std::marker::PhantomData,
                           }, data
                       }
                   }
               }

               // we also generate a conversion from typed to untyped, which
               // we use to write convenience methods on the wrapper
               impl<'a, #t> #raw_name<'a, #t> {
                   #[allow(dead_code)]
                   #[doc = #of_unit_docs]
                   pub(crate) fn of_unit_type(&self) -> #raw_name<'a, ()> {
                       let TableRef { data, #shape_name} = self;
                       TableRef {
                           shape: #marker_name {
                               #( #shape_fields, )*
                               offset_type: std::marker::PhantomData,
                           }, data: *data,
                       }
                   }
               }
        }
    });

    let table_ref_getters = item.iter_table_ref_getters();

    let optional_format_trait_impl = item.impl_format_trait();
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

    Ok(quote! {
        #optional_format_trait_impl

        #( #docs )*
        #[derive(Debug, #derive_clone_copy)]
        #[doc(hidden)]
        pub struct #marker_name <#generic_with_default> {
            #( #shape_fields, )*
            #phantom_decl
        }

        impl <#generic> #marker_name <#generic> {
            #( #shape_byte_range_fns )*
        }

        #optional_min_byte_range_trait_impl

        #top_level

        #impl_clone_copy

        #font_read

        #impl_into_generic

        #( #docs )*
        pub type #raw_name<'a, #generic> = TableRef<'a, #marker_name<#generic>>;

        #[allow(clippy::needless_lifetimes)]
        impl<'a, #generic> #raw_name<'a, #generic> {

            #( #table_ref_getters )*

        }

        #debug
    })
}

fn generate_font_read(item: &Table) -> syn::Result<TokenStream> {
    let marker_name = item.marker_name();
    let name = item.raw_name();
    let field_validation_stmts = item.iter_field_validation_stmts();
    let shape_field_names = item.iter_shape_field_names();
    let generic = item.attrs.generic_offset.as_ref();
    let phantom = generic.map(|_| quote!(offset_type: std::marker::PhantomData,));
    let error_if_phantom_and_read_args = generic.map(|_| {
        quote!(compile_error!(
            "ReadWithArgs not implemented for tables with phantom params."
        );)
    });

    // the cursor doesn't need to be mut if there are no fields,
    // which happens at least once (in glyf)?
    let maybe_mut_kw = (!item.fields.fields.is_empty()).then(|| quote!(mut));

    if let Some(read_args) = &item.attrs.read_args {
        let args_type = read_args.args_type();
        let destructure_pattern = read_args.destructure_pattern();
        let constructor_args = read_args.constructor_args();
        let args_from_constructor_args = read_args.read_args_from_constructor_args();
        Ok(quote! {
            #error_if_phantom_and_read_args
            impl ReadArgs for #name<'_> {
                type Args = #args_type;
            }

            impl<'a> FontReadWithArgs<'a> for #name<'a> {
                fn read_with_args(data: FontData<'a>, args: &#args_type) -> Result<Self, ReadError> {
                    let #destructure_pattern = *args;
                    let #maybe_mut_kw cursor = data.cursor();
                    #( #field_validation_stmts )*
                    cursor.finish( #marker_name {
                        #( #shape_field_names, )*
                    })
                }
            }

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
        })
    } else {
        Ok(quote! {
            impl<'a, #generic> FontRead<'a> for #name<'a, #generic> {
            fn read(data: FontData<'a>) -> Result<Self, ReadError> {
                let #maybe_mut_kw cursor = data.cursor();
                #( #field_validation_stmts )*
                cursor.finish( #marker_name {
                    #( #shape_field_names, )*
                    #phantom
                })
            }
        }
        })
    }
}

pub(crate) fn generate_group(item: &GenericGroup) -> syn::Result<TokenStream> {
    let docs = &item.attrs.docs;
    let name = &item.name;
    let inner = &item.inner_type;
    let type_field = &item.inner_field;

    let mut variant_decls = Vec::new();
    let mut read_match_arms = Vec::new();
    let mut dyn_inner_arms = Vec::new();
    let mut of_unit_arms = Vec::new();
    for var in &item.variants {
        let var_name = &var.name;
        let type_id = &var.type_id;
        let typ = &var.typ;
        variant_decls.push(quote! { #var_name ( #inner <'a, #typ<'a>> ) });
        read_match_arms
            .push(quote! { #type_id => Ok(#name :: #var_name (untyped.into_concrete())) });
        dyn_inner_arms.push(quote! { #name :: #var_name(table) => table });
        of_unit_arms.push(quote! { #name :: #var_name(inner) => inner.of_unit_type()  });
    }

    let of_unit_docs = &[
        " Return the inner table, removing the specific generics.",
        "",
        " This lets us return a single concrete type we can call methods on.",
    ];

    Ok(quote! {
        #( #docs)*
        pub enum #name <'a> {
            #( #variant_decls, )*
        }

        impl<'a> FontRead<'a> for #name <'a> {
            fn read(bytes: FontData<'a>) -> Result<Self, ReadError> {
                let untyped = #inner::read(bytes)?;
                match untyped.#type_field() {
                    #( #read_match_arms, )*
                    other => Err(ReadError::InvalidFormat(other.into())),
                }
            }
        }

        impl<'a> #name <'a> {
            #[allow(dead_code)]
            #(  #[doc = #of_unit_docs] )*
            pub(crate) fn of_unit_type(&self) -> #inner<'a, ()> {
                match self {
                    #( #of_unit_arms, )*
                }
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> #name <'a> {
            fn dyn_inner(&self) -> &(dyn SomeTable<'a> + 'a) {
                match self {
                    #( #dyn_inner_arms, )*
                }
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> SomeTable<'a> for #name <'a> {

            fn get_field(&self, idx: usize) -> Option<Field<'a>> {
                self.dyn_inner().get_field(idx)
            }

            fn type_name(&self) -> &str {
                self.dyn_inner().type_name()
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl std::fmt::Debug for #name<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.dyn_inner().fmt(f)
            }
        }
    })
}

fn generate_debug(item: &Table) -> syn::Result<TokenStream> {
    let name = item.raw_name();
    let name_str = name.to_string();
    let generic = item.attrs.generic_offset.as_ref();
    let generic_bounds = generic
        .is_some()
        .then(|| quote!(: FontRead<'a> + SomeTable<'a> + 'a));
    let version = item.fields.version_field().map(|fld| {
        let name = &fld.name;
        quote!(let version = self.#name();)
    });
    let condition_inputs = item
        .fields
        .conditional_input_idents()
        .into_iter()
        .map(|fld| quote!( let #fld = self.#fld(); ));
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
                #version
                #( #condition_inputs )*
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
                U: FontRead<'a>,
                #t: FromTableRef<U> + Default + 'static,
        }
    });

    let impl_font_read = item.attrs.read_args.is_none() && item.attrs.generic_offset.is_none();
    let maybe_font_read = impl_font_read.then(|| {
        quote! {
            impl<'a> FontRead<'a> for #name {
                fn read(data: FontData<'a>) -> Result<Self, ReadError> {
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

pub(crate) fn generate_group_compile(
    item: &GenericGroup,
    parse_module: &syn::Path,
) -> syn::Result<TokenStream> {
    let docs = &item.attrs.docs;
    let name = &item.name;
    let inner = &item.inner_type;

    let mut variant_decls = Vec::new();
    let mut write_match_arms = Vec::new();
    let mut validate_match_arms = Vec::new();
    let mut from_obj_match_arms = Vec::new();
    let mut type_arms = Vec::new();
    let mut from_impls = Vec::new();
    let from_type = quote!(#parse_module :: #name);
    for var in &item.variants {
        let var_name = &var.name;
        let typ = &var.typ;

        variant_decls.push(quote! { #var_name ( #inner <#typ> ) });
        write_match_arms.push(quote! { Self :: #var_name (table) => table.write_into(writer)  });
        validate_match_arms.push(quote! { Self :: #var_name(table) => table.validate_impl(ctx) });
        from_obj_match_arms.push(
            quote! { #from_type :: #var_name(table) => Self :: #var_name(table.to_owned_obj(data)) },
        );
        type_arms.push(quote! { Self:: #var_name(table) => table.table_type()  });
        from_impls.push(quote! {
            impl From<#inner <#typ>> for #name {
                fn from(src: #inner <#typ>) -> #name {
                    #name :: #var_name ( src )
                }
            }
        });
    }
    let first_var_name = &item.variants.first().unwrap().name;

    Ok(quote! {
        #( #docs)*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum #name {
            #( #variant_decls, )*
        }

        impl Default for #name {
            fn default() -> Self {
                Self::#first_var_name(Default::default())
            }
        }

        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                match self {
                    #( #write_match_arms, )*
                }
            }

            fn table_type(&self) -> TableType {
                match self {
                    #( #type_arms, )*
                }
            }
        }

        impl Validate for #name {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                match self {
                    #( #validate_match_arms, )*
                }
            }
        }

        impl FromObjRef< #from_type :: <'_>> for #name {
            fn from_obj_ref(from: & #from_type :: <'_>, data: FontData) -> Self {
                match from {
                    #( #from_obj_match_arms, )*
                }
            }
        }

        impl FromTableRef< #from_type <'_>> for #name {}

        #( #from_impls )*

    })
}

pub(crate) fn generate_format_compile(
    item: &TableFormat,
    items: &Items,
) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let parse_module = &items.parse_module_path;
    let variants = item.variants.iter().map(|variant| {
        let name = &variant.name;
        let typ = variant.type_name();
        let docs = &variant.attrs.docs;
        quote! ( #( #docs )* #name(#typ) )
    });

    let default_variant = &item.variants.first().unwrap().name;

    let write_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.write_into(writer), )
    });

    let validation_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.validate_impl(ctx), )
    });

    let table_type_arms = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        quote!( Self::#var_name(item) => item.table_type(), )
    });

    let from_impls = item.variants.iter().map(|variant| {
        let var_name = &variant.name;
        let typ = variant.type_name();
        quote!( impl From<#typ> for #name {
            fn from(src: #typ) -> #name {
                #name::#var_name(src)
            }
        } )
    });

    let from_obj_impl = item
        .attrs
        .skip_from_obj
        .is_none()
        .then(|| generate_format_from_obj(item, parse_module))
        .transpose()?;

    let constructors = generate_format_constructors(item, items)?;
    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum #name {
            #( #variants ),*
        }

        #constructors

        impl Default for #name {
            fn default() -> Self {
                Self::#default_variant(Default::default())
            }
        }

        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                match self {
                    #( #write_arms )*
                }
            }

            fn table_type(&self) -> TableType {
                match self {
                    #( #table_type_arms )*
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

        #from_obj_impl

        #( #from_impls )*

    })
}

fn generate_format_constructors(item: &TableFormat, items: &Items) -> syn::Result<TokenStream> {
    let mut constructors = Vec::new();
    let name = &item.name;

    for variant in &item.variants {
        let var_name = &variant.name;
        let var_type = variant.type_name();

        let Some(Item::Table(table)) = items.get(var_type) else {
            return Err(logged_syn_error(var_type.span(), "Unknown type; codegen currently expects types in format groups to be local to the file."));
        };
        if table.attrs.skip_constructor.is_some() {
            continue;
        }

        let constructor_args_raw = table.fields.iter_constructor_info().collect::<Vec<_>>();
        let constructor_args = constructor_args_raw.iter().map(
            |FieldConstructorInfo {
                 name, arg_tokens, ..
             }| quote!(#name: #arg_tokens),
        );
        let constructor_arg_names = constructor_args_raw.iter().map(|info| &info.name);

        let constructor_ident = make_snake_case_ident(var_name);

        let docstring = format!(" Construct a new `{}` subtable", variant.type_name());
        // judiciously allow this lint
        let too_many_args =
            (constructor_args.len() > 7).then(|| quote!(#[allow(clippy::too_many_arguments)]));
        constructors.push(quote! {
             #[doc = #docstring]
            #too_many_args
            pub fn #constructor_ident ( #( #constructor_args,)*  ) -> Self {
                Self::#var_name( #var_type::new( #( #constructor_arg_names, )* ))
            }
        });
    }

    Ok(quote! {
        impl #name {

            #( #constructors )*
        }
    })
}

fn generate_format_shared_getters(item: &TableFormat, items: &Items) -> syn::Result<TokenStream> {
    // okay so we want to identify the getters that exist on all variants.
    let all_variants = item
        .variants
        .iter()
        .map(|var| {
            let type_name = var.type_name();
            match items.get(type_name) {
                Some(Item::Table(item)) => Ok(item),
                _ => Err(logged_syn_error(
                    type_name.span(),
                    "must be a table defined in this file",
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    // okay so now we have all of the actual inner types, and we need to find which
    // getters are shared between all of them
    let mut field_counts = IndexMap::new();
    let mut all_fields = HashMap::new();
    for table in &all_variants {
        for field in table.fields.iter().filter(|fld| fld.has_getter()) {
            let key = (&field.name, &field.typ);
            // we have to convert the tokens to a string to get hash/ord/etc
            *field_counts.entry(key).or_insert(0usize) += 1;
            all_fields.entry(&field.name).or_insert(field);
        }
    }

    let shared_fields = field_counts
        .into_iter()
        .filter(|(_, count)| *count == all_variants.len())
        .map(|((name, _), _)| all_fields.get(name).unwrap())
        .collect::<Vec<_>>();

    let getters = shared_fields
        .iter()
        .map(|fld| generate_format_getter_for_shared_field(item, fld));

    // and we also want to have a wrapper for offset_data():
    let data_arms = item
        .variants
        .iter()
        .filter(|v| v.attrs.write_only.is_none())
        .map(|variant| {
            let var_name = &variant.name;
            quote!(Self::#var_name(item) => item.offset_data(), )
        });

    // now we have a collection of fields present on all variants, and
    // we need to actually generate the wrapping getter

    Ok(quote! {
        #[doc = "Return the `FontData` used to resolve offsets for this table."]
        pub fn offset_data(&self) -> FontData<'a> {
            match self {
                #( #data_arms )*
            }
        }
        #( #getters )*
    })
}

fn generate_format_getter_for_shared_field(item: &TableFormat, field: &Field) -> TokenStream {
    let docs = &field.attrs.docs;
    let method_name = &field.name;
    let return_type = field.table_getter_return_type();
    let arms = item.variants.iter().map(|variant| {
        let var_name: &syn::Ident = &variant.name;
        quote!(Self::#var_name(item) => item.#method_name(), )
    });

    // but we also need to handle offset getters, and that's a pain

    quote! {
        #( #docs )*
        pub fn #method_name(&self) -> #return_type {
            match self {
                #( #arms )*
            }
        }
    }
}

fn generate_format_from_obj(
    item: &TableFormat,
    parse_module: &syn::Path,
) -> syn::Result<TokenStream> {
    let name = &item.name;
    let to_owned_arms = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let var_name = &variant.name;
            quote!( ObjRefType::#var_name(item) => #name::#var_name(item.to_owned_table()), )
        });

    Ok(quote! {
        impl FromObjRef<#parse_module:: #name<'_>> for #name {
            fn from_obj_ref(obj: &#parse_module:: #name, _: FontData) -> Self {
                use #parse_module::#name as ObjRefType;
                match obj {
                    #( #to_owned_arms )*
                }
            }
        }

        impl FromTableRef<#parse_module::#name<'_>> for #name {}

        impl<'a> FontRead<'a> for #name {
            fn read(data: FontData<'a>) -> Result<Self, ReadError> {
                <#parse_module :: #name as FontRead>::read(data)
                    .map(|x| x.to_owned_table())
            }
        }
    })
}

pub(crate) fn generate_format_group(item: &TableFormat, items: &Items) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let variants = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let name = &variant.name;
            let typ = variant.type_name();
            let docs = &variant.attrs.docs;
            quote! ( #( #docs )* #name(#typ<'a>) )
        });

    let format = &item.format;
    // if we have any fancy match statement we disable a clippy lint
    let mut has_any_match_stmt = false;
    let match_arms = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let name = &variant.name;
            let lhs = if let Some(expr) = variant.attrs.match_stmt.as_deref() {
                has_any_match_stmt = true;
                let expr = &expr.expr;
                quote!(format if #expr)
            } else {
                let typ = variant.marker_name();
                quote!(#typ::FORMAT)
            };
            Some(quote! {
                #lhs => {
                    Ok(Self::#name(FontRead::read(data)?))
                }
            })
        })
        .collect::<Vec<_>>();

    let maybe_allow_lint = has_any_match_stmt.then(|| quote!(#[allow(clippy::redundant_guards)]));

    let traversal_arms = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let name = &variant.name;
            quote!(Self::#name(table) => table)
        });

    let format_offset = item
        .format_offset
        .as_ref()
        .map(|lit| lit.base10_parse::<usize>().unwrap())
        .unwrap_or(0);

    let getters = generate_format_shared_getters(item, items)?;
    let getters = (!getters.is_empty()).then(|| {
        quote! {
            impl<'a> #name<'a> {
                #getters
            }
        }
    });

    let min_byte_arms = item
        .variants
        .iter()
        .filter(|variant| variant.attrs.write_only.is_none())
        .map(|variant| {
            let var_name: &syn::Ident = &variant.name;
            quote!(Self::#var_name(item) => item.min_byte_range(), )
        });

    Ok(quote! {
        #( #docs )*
        #[derive(Clone)]
        pub enum #name<'a> {
            #( #variants ),*
        }

        #getters

        impl<'a> FontRead<'a> for #name<'a> {
            fn read(data: FontData<'a>) -> Result<Self, ReadError> {
                let format: #format = data.read_at(#format_offset)?;
                #maybe_allow_lint
                match format {
                    #( #match_arms ),*
                    other => Err(ReadError::InvalidFormat(other.into())),
                }
            }
        }

        impl MinByteRange for #name<'_> {
            fn min_byte_range(&self) -> Range<usize> {
                match self {
                    #( #min_byte_arms )*
                }
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> #name<'a> {
            fn dyn_inner<'b>(&'b self) -> &'b dyn SomeTable<'a> {
                match self {
                    #( #traversal_arms, )*
                }
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl std::fmt::Debug for #name<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.dyn_inner().fmt(f)
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> SomeTable<'a> for #name<'a> {
            fn type_name(&self) -> &str {
                self.dyn_inner().type_name()
            }

            fn get_field(&self, idx: usize) -> Option<Field<'a>> {
                self.dyn_inner().get_field(idx)
            }
        }
    })
}

impl Table {
    pub(crate) fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        self.fields.sanity_check(phase)
    }

    fn marker_name(&self) -> syn::Ident {
        quote::format_ident!("{}Marker", self.raw_name())
    }

    fn iter_shape_byte_fns(&self) -> impl Iterator<Item = TokenStream> + '_ {
        let mut prev_field_end_expr = quote!(0);
        let mut iter = self.fields.iter();

        std::iter::from_fn(move || {
            let field = iter.next()?;
            let fn_name = field.shape_byte_range_fn_name();
            let len_expr = field.shape_len_expr();

            // versioned fields have a different signature
            if field.attrs.conditional.is_some() {
                prev_field_end_expr = quote! {
                    self.#fn_name().map(|range| range.end)
                        .unwrap_or_else(|| #prev_field_end_expr)
                };
                let start_field_name = field.shape_byte_start_field_name();
                return Some(quote! {
                    pub fn #fn_name(&self) -> Option<Range<usize>> {
                        let start = self.#start_field_name?;
                        Some(start..start + #len_expr)
                    }
                });
            }

            let result = quote! {
                pub fn #fn_name(&self) -> Range<usize> {
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
        // if an input arg is needed later, save it in the shape.
        if let Some(args) = &self.attrs.read_args {
            result.extend(
                args.args
                    .iter()
                    .filter(|arg| self.fields.referenced_fields.needs_at_runtime(&arg.ident))
                    .map(|arg| (arg.ident.clone(), arg.typ.to_token_stream())),
            );
        }

        for next in self.fields.iter() {
            let is_versioned = next.attrs.conditional.is_some();
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
        let generic = self.attrs.generic_offset.as_ref().map(|attr| &attr.attr);
        self.fields
            .iter()
            .filter_map(move |fld| fld.table_getter(generic))
            .chain(
                self.attrs
                    .read_args
                    .as_ref()
                    .into_iter()
                    .flat_map(|args| args.iter_table_ref_getters(&self.fields.referenced_fields)),
            )
    }

    pub(crate) fn impl_format_trait(&self) -> Option<TokenStream> {
        let field = self.fields.iter().find(|fld| fld.attrs.format.is_some())?;
        let name = self.marker_name();
        let value = &field.attrs.format.as_ref().unwrap();
        let typ = field.typ.cooked_type_tokens();

        Some(quote! {
            impl Format<#typ> for #name {
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
        let name = self.marker_name();

        let fn_name = field.shape_byte_range_fn_name();
        Some(quote! {
            impl MinByteRange for #name {
                fn min_byte_range(&self) -> Range<usize> {
                    0..self.#fn_name().end
                }
            }
        })
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
            [TableReadArg { ident, .. }] => ident.to_token_stream(),
            other => {
                let idents = other.iter().map(|arg| &arg.ident);
                quote!( ( #(#idents,)* ) )
            }
        }
    }

    pub(crate) fn constructor_args(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.args
            .iter()
            .map(|TableReadArg { ident, typ }| quote!(#ident: #typ))
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

    fn iter_table_ref_getters<'a>(
        &'a self,
        referenced_fields: &'a ReferencedFields,
    ) -> impl Iterator<Item = TokenStream> + 'a {
        self.args
            .iter()
            .filter(|arg| referenced_fields.needs_at_runtime(&arg.ident))
            .map(|TableReadArg { ident, typ }| {
                quote! {
                    pub(crate) fn #ident(&self) -> #typ {
                        self.shape.#ident
                    }
                }
            })
    }
}

// An overwrought and likely incorrect way of converting 'Format1' to 'format_1' -_-
fn make_snake_case_ident(ident: &syn::Ident) -> syn::Ident {
    let input = ident.to_string();
    let mut output = String::with_capacity(input.len() + 2);
    let mut prev_char = input.chars().next().unwrap();
    output.extend(prev_char.to_lowercase());
    for c in input.chars().skip(1) {
        if (c.is_uppercase() && !prev_char.is_uppercase())
            || (c.is_numeric() && !prev_char.is_numeric())
        {
            output.push('_');
        }
        output.extend(c.to_lowercase());
        prev_char = c;
    }

    syn::Ident::new(&output, ident.span())
}
