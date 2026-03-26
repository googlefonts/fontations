//! Code generation for `ReadSanitized` types.
//!
//! These are high-efficiency types for reading pre-validated font data without
//! bounds checking, using raw pointer arithmetic via `FontPtr`.

use std::collections::HashMap;

use indexmap::IndexMap;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};

use crate::parsing::{
    logged_syn_error, Count, Field, FieldReadArgs, FieldType, Fields, FormatVariant, GenericGroup,
    Item, Items, OffsetTarget, Record, Table, TableFormat,
};

/// Generate a `ReadSanitized` implementation for a table.
pub(crate) fn generate_read_sanitized_table(
    item: &Table,
    items: &Items,
) -> syn::Result<TokenStream> {
    if item.attrs.write_only.is_some() {
        return Ok(Default::default());
    }

    let name = item.raw_name();
    let sanitized_name = format_ident!("{}Sanitized", name);
    let generic = item.attrs.generic_offset.as_ref().map(|a| &a.attr);
    let generic_with_default = generic.map(|t| quote!(#t = ()));
    let phantom_decl =
        generic.map(|t| quote!(pub(crate) phantom: std::marker::PhantomData<*const #t>,));
    let phantom_init = generic.map(|_| quote!(phantom: std::marker::PhantomData,));
    // struct uses `<'a, T = ()>` or `<'a>`, impl uses `<'a, T>` or `<'a>`,
    // ReadSanitized targets `Name<'a, ()>` or `Name<'a>`.
    let struct_generics = generic_with_default
        .as_ref()
        .map_or(quote!(<'a>), |g| quote!(<'a, #g>));
    let impl_generics = generic.as_ref().map_or(quote!(<'a>), |g| quote!(<'a, #g>));
    let rs_self_type = quote!(#sanitized_name<'a, #generic>);

    // Determine ReadSanitized::Args type and extra struct fields from #[read_args].
    let (args_type, extra_struct_fields, args_init_stmts, arg_getters) =
        build_args_info(&item.fields.read_args);

    let args_param = if item.fields.read_args.is_some() {
        quote!(args: &Self::Args)
    } else {
        quote!(_args: &Self::Args)
    };

    let read_sanitized_init =
        build_read_sanitized_init(&item.fields.read_args, phantom_init.as_ref());

    let (pos_methods, getter_methods) = build_pos_and_getter_methods(&item.fields, items);

    let impl_into_generic = generic.map(|t| {
        quote! {
            impl<'a> #sanitized_name<'a, ()> {
                #[allow(dead_code)]
                pub(crate) fn into_concrete<#t>(self) -> #sanitized_name<'a, #t> {
                    #sanitized_name { ptr: self.ptr, phantom: std::marker::PhantomData }
                }
            }
            impl<'a, #t> #sanitized_name<'a, #t> {
                #[allow(dead_code)]
                pub(crate) fn of_unit_type(&self) -> #sanitized_name<'a, ()> {
                    #sanitized_name { ptr: self.ptr.clone(), phantom: std::marker::PhantomData }
                }
            }
        }
    });

    Ok(quote! {
        #[derive(Clone, Default)]
        pub struct #sanitized_name #struct_generics {
            pub(crate) ptr: FontPtr<'a>,
            #phantom_decl
            #(#extra_struct_fields,)*
        }

        impl #impl_generics #sanitized_name #impl_generics {
            pub fn offset_ptr(&self) -> FontPtr<'a> { self.ptr }
            #(#pos_methods)*
            #(#arg_getters)*
            #(#getter_methods)*
        }

        #impl_into_generic

        unsafe impl #impl_generics ReadSanitized<'a> for #rs_self_type {
            type Args = #args_type;

            unsafe fn read_sanitized(ptr: FontPtr<'a>, #args_param) -> Self {
                #(#args_init_stmts)*
                #read_sanitized_init
            }
        }
    })
}

/// Generate a pointer-based `ReadSanitized` struct for a record that cannot be
/// represented as a plain zerocopy struct — i.e. it has `#[read_args]` and
/// either a lifetime (array fields) or variable-size struct fields (like `ValueRecord`).
///
/// The generated struct stores `ptr: FontPtr<'a>` plus one field per `#[read_args]`
/// argument, and uses the same `_pos()` / getter pattern as table sanitized types.
fn generate_ptr_based_read_sanitized_record(
    item: &Record,
    items: &Items,
) -> syn::Result<TokenStream> {
    let name = &item.name;
    let sanitized_name = format_ident!("{}Sanitized", name);

    let (args_type, extra_struct_fields, args_init_stmts, arg_getters) =
        build_args_info(&item.fields.read_args);

    let args_param = if item.fields.read_args.is_some() {
        quote!(args: &Self::Args)
    } else {
        quote!(_args: &Self::Args)
    };

    let read_sanitized_init = build_read_sanitized_init(&item.fields.read_args, None);

    let (pos_methods, getter_methods) = build_pos_and_getter_methods(&item.fields, items);

    Ok(quote! {
        #[derive(Clone, Default)]
        pub struct #sanitized_name<'a> {
            pub(crate) ptr: FontPtr<'a>,
            #(#extra_struct_fields,)*
        }

        impl<'a> #sanitized_name<'a> {
            pub fn offset_ptr(&self) -> FontPtr<'a> { self.ptr }
            #(#pos_methods)*
            #(#arg_getters)*
            #(#getter_methods)*
        }

        unsafe impl<'a> ReadSanitized<'a> for #sanitized_name<'a> {
            type Args = #args_type;

            unsafe fn read_sanitized(ptr: FontPtr<'a>, #args_param) -> Self {
                #(#args_init_stmts)*
                #read_sanitized_init
            }
        }
    })
}

/// Generate a `ReadSanitized`-style struct for a record type.
///
/// Records with `#[read_args]` that cannot be represented as a plain zerocopy struct
/// (because they have a lifetime or variable-size struct fields) get a pointer-based
/// struct instead. All other records with `#[read_args]` or only scalar/offset/struct
/// fields get a zerocopy `#[repr(C, packed)]` struct.
pub(crate) fn generate_read_sanitized_record(
    item: &Record,
    items: &Items,
) -> syn::Result<TokenStream> {
    let has_unsupported_struct = item
        .fields
        .iter()
        .any(|f| matches!(&f.typ, FieldType::Struct { typ } if !has_sanitized_record(typ, items)));

    // Records with read_args that can't be zerocopy get a pointer-based sanitized struct.
    if item.fields.read_args.is_some() && (item.lifetime.is_some() || has_unsupported_struct) {
        return generate_ptr_based_read_sanitized_record(item, items);
    }

    // Skip records with lifetimes (they contain arrays without read_args).
    if item.lifetime.is_some() {
        return Ok(Default::default());
    }

    // Skip records that contain embedded struct fields whose type doesn't have
    // a sanitized version (e.g. extern types, variable-size records).
    if has_unsupported_struct {
        return Ok(Default::default());
    }

    let name = &item.name;
    let sanitized_name = format_ident!("{}Sanitized", name);
    let docs = &item.attrs.docs;

    let mut field_decls: Vec<TokenStream> = Vec::new();
    let mut field_getters: Vec<TokenStream> = Vec::new();
    let mut offset_methods: Vec<TokenStream> = Vec::new();
    let mut all_zerocopy = true;

    for field in item.fields.iter() {
        if field.attrs.skip_getter.is_some() {
            continue;
        }
        match &field.typ {
            FieldType::Array { .. } | FieldType::ComputedArray(_) | FieldType::VarLenArray(_) => {
                // Records with arrays have lifetimes and are filtered above.
                // This path shouldn't be hit, but handle gracefully.
                all_zerocopy = false;
                continue;
            }
            FieldType::PendingResolution { .. } => {
                panic!("unresolved field type in generate_read_sanitized_record")
            }
            _ => {}
        }
        field_decls.push(field.sanitized_record_field_decl());
        field_getters.push(field.sanitized_record_getter());
        if let Some(method) = field.sanitized_record_offset_method(items) {
            offset_methods.push(method);
        }
    }

    if !all_zerocopy {
        return Ok(Default::default());
    }

    // Compute RAW_BYTE_LEN from all field types.
    let raw_field_types: Vec<TokenStream> = item
        .fields
        .iter()
        .filter(|f| f.attrs.skip_getter.is_none())
        .map(|f| {
            let typ = f.typ.cooked_type_tokens();
            quote!(#typ::RAW_BYTE_LEN)
        })
        .collect();

    // Since all fields are either Scalar (BigEndian<T>) or Offset (BigEndian<OffsetN>),
    // which are both AnyBitPattern + Copy, we can always derive these.
    let extra_derives = quote!(Copy, bytemuck::AnyBitPattern,);

    let impl_block = if field_getters.is_empty() && offset_methods.is_empty() {
        quote!()
    } else {
        quote! {
            impl #sanitized_name {
                #(#field_getters)*
                #(#offset_methods)*
            }
        }
    };

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Debug, #extra_derives)]
        #[repr(C)]
        #[repr(packed)]
        pub struct #sanitized_name {
            #( #field_decls, )*
        }

        impl FixedSize for #sanitized_name {
            const RAW_BYTE_LEN: usize = #( #raw_field_types )+*;
        }

        #impl_block
    })
}

/// Generate sanitized getters for fields shared across all active variants of a format group.
fn generate_sanitized_format_shared_getters(
    active_variants: &[&FormatVariant],
    items: &Items,
) -> syn::Result<TokenStream> {
    let all_tables = active_variants
        .iter()
        .map(|v| {
            let type_name = v.type_name();
            match items.get(type_name) {
                Some(Item::Table(t)) => Ok(t),
                _ => Err(logged_syn_error(
                    type_name.span(),
                    "must be a table defined in this file",
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut field_counts: IndexMap<(&syn::Ident, &FieldType), usize> = IndexMap::new();
    let mut all_fields: HashMap<&syn::Ident, &Field> = HashMap::new();
    for table in &all_tables {
        for field in table.fields.iter().filter(|f| f.has_getter()) {
            let key = (&field.name, &field.typ);
            *field_counts.entry(key).or_insert(0) += 1;
            all_fields.entry(&field.name).or_insert(field);
        }
    }

    let shared: Vec<&Field> = field_counts
        .iter()
        .filter(|(_, &count)| count == all_tables.len())
        .map(|((name, _), _)| *all_fields.get(*name).unwrap())
        .collect();

    let getters = shared.iter().map(|field| {
        let docs = &field.attrs.docs;
        let method_name = &field.name;
        let return_type = field.sanitized_table_getter_return_type(items);
        let arms = active_variants.iter().map(|v| {
            let var_name = &v.name;
            quote!(Self::#var_name(item) => item.#method_name(),)
        });
        quote! {
            #( #docs )*
            pub fn #method_name(&self) -> #return_type {
                match self {
                    #( #arms )*
                }
            }
        }
    });

    Ok(quote! { #(#getters)* })
}

/// Generate a `ReadSanitized` enum for a format-dispatch table group.
pub(crate) fn generate_read_sanitized_format(
    item: &TableFormat,
    items: &Items,
) -> syn::Result<TokenStream> {
    let active_variants: Vec<_> = item
        .variants
        .iter()
        .filter(|v| v.attrs.write_only.is_none())
        .collect();

    let name = &item.name;
    let sanitized_name = format_ident!("{}Sanitized", name);
    let format_type = &item.format;
    let format_offset: usize = item
        .format_offset
        .as_ref()
        .and_then(|lit| lit.base10_parse().ok())
        .unwrap_or(0usize);

    let variant_defs: Vec<_> = active_variants
        .iter()
        .map(|v| {
            let var_name = &v.name;
            let typ_s = format_ident!("{}Sanitized", v.type_name());
            quote!(#var_name(#typ_s<'a>))
        })
        .collect();

    let first_var = &active_variants[0].name;
    let first_typ_s = format_ident!("{}Sanitized", active_variants[0].type_name());

    let mut has_match_stmt = false;
    let match_arms: Vec<_> = active_variants
        .iter()
        .map(|v| {
            let var_name = &v.name;
            let typ = v.type_name();
            let lhs = if let Some(expr) = v.attrs.match_stmt.as_deref() {
                has_match_stmt = true;
                let e = &expr.expr;
                quote!(format if #e)
            } else {
                quote!(#typ::FORMAT)
            };
            quote!(#lhs => Self::#var_name(ReadSanitized::read_sanitized(ptr, &())))
        })
        .collect();
    let maybe_allow_lint = has_match_stmt.then(|| quote!(#[allow(clippy::redundant_guards)]));

    let ptr_arms = active_variants.iter().map(|v| {
        let var_name = &v.name;
        quote!(Self::#var_name(item) => item.offset_ptr(),)
    });

    let shared_getters = generate_sanitized_format_shared_getters(&active_variants, items)?;

    Ok(quote! {
        #[derive(Clone)]
        pub enum #sanitized_name<'a> {
            #( #variant_defs ),*
        }

        impl<'a> #sanitized_name<'a> {
            pub fn offset_ptr(&self) -> FontPtr<'a> {
                match self {
                    #( #ptr_arms )*
                }
            }
            #shared_getters
        }

        impl<'a> Default for #sanitized_name<'a> {
            fn default() -> Self {
                Self::#first_var(#first_typ_s::default())
            }
        }

        unsafe impl<'a> ReadSanitized<'a> for #sanitized_name<'a> {
            type Args = ();
            unsafe fn read_sanitized(ptr: FontPtr<'a>, _args: &()) -> Self {
                let format: #format_type = ptr.read_at(#format_offset);
                #maybe_allow_lint
                match format {
                    #( #match_arms, )*
                    _ => Self::default(),
                }
            }
        }
    })
}

/// Generate a `ReadSanitized` enum for a generic-group table.
///
/// Each variant stores the fully-typed `#inner_sanitized<'a, #typ_sanitized<'a>>`,
/// mirroring how `generate_group` stores `#inner<'a, #typ<'a>>`.  The dispatch
/// field is read by constructing a unit-typed inner sanitized and calling its
/// getter, so no manual byte-offset computation is required.
pub(crate) fn generate_read_sanitized_group(item: &GenericGroup) -> syn::Result<TokenStream> {
    let name = &item.name;
    let inner_type = &item.inner_type;
    let inner_field = &item.inner_field;
    let sanitized_name = format_ident!("{}Sanitized", name);
    let inner_sanitized = format_ident!("{}Sanitized", inner_type);
    let first_var = &item.variants[0].name;

    let variant_defs: Vec<_> = item
        .variants
        .iter()
        .map(|v| {
            let n = &v.name;
            let typ_sanitized = format_ident!("{}Sanitized", v.typ);
            quote!(#n(#inner_sanitized<'a, #typ_sanitized<'a>>))
        })
        .collect();

    let match_arms: Vec<_> = item
        .variants
        .iter()
        .map(|v| {
            let n = &v.name;
            let id = &v.type_id;
            quote!(#id => Self::#n(ReadSanitized::read_sanitized(ptr, &())))
        })
        .collect();

    let ptr_arms = item.variants.iter().map(|v| {
        let n = &v.name;
        quote!(Self::#n(item) => item.offset_ptr(),)
    });

    Ok(quote! {
        #[derive(Clone)]
        pub enum #sanitized_name<'a> {
            #( #variant_defs, )*
        }

        impl<'a> #sanitized_name<'a> {
            pub fn offset_ptr(&self) -> FontPtr<'a> {
                match self {
                    #( #ptr_arms )*
                }
            }
        }

        impl<'a> Default for #sanitized_name<'a> {
            fn default() -> Self {
                Self::#first_var(Default::default())
            }
        }

        unsafe impl<'a> ReadSanitized<'a> for #sanitized_name<'a> {
            type Args = ();
            unsafe fn read_sanitized(ptr: FontPtr<'a>, _args: &()) -> Self {
                let untyped: #inner_sanitized<'a, ()> = ReadSanitized::read_sanitized(ptr, &());
                let type_id = untyped.#inner_field();
                match type_id {
                    #( #match_arms, )*
                    _ => Self::default(),
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build `(pos_methods, getter_methods)` by walking fields in order.
///
/// Used by both table and pointer-based record generation.  For each field:
/// - A private `field_pos()` method is emitted that returns the byte offset of
///   that field within the struct's data.  It chains off the previous field's
///   position method, accumulating any bytes belonging to skipped fields.
/// - A public getter method is emitted that reads the field value via the pos
///   method.  Offset fields additionally get a resolved offset getter.
fn build_pos_and_getter_methods(
    fields: &Fields,
    items: &Items,
) -> (Vec<TokenStream>, Vec<TokenStream>) {
    let mut pos_methods: Vec<TokenStream> = Vec::new();
    let mut getter_methods: Vec<TokenStream> = Vec::new();

    // `prev_fn`: ident of the last emitted _pos() method (None = start of struct).
    // `acc`: accumulated byte count since `prev_fn` (None means 0).
    let mut prev_fn: Option<syn::Ident> = None;
    let mut acc: Option<TokenStream> = None;

    for field in fields.iter() {
        let this_len = field.sanitized_byte_len().unwrap();

        if field.attrs.skip_getter.is_some() {
            // Don't generate getter/pos, but account for this field's bytes.
            acc = Some(match acc.take() {
                None => this_len,
                Some(prev) => quote!(#prev + #this_len),
            });
            continue;
        }

        let field_name = &field.name;
        let pos_fn_ident = format_ident!("{}_pos", field_name);

        let pos_body = match (&prev_fn, &acc) {
            (None, None) => quote!(0),
            (None, Some(offset)) => quote!(#offset),
            (Some(f), None) => quote!(self.#f()),
            (Some(f), Some(a)) => quote!(self.#f() + #a),
        };
        pos_methods.push(quote! {
            fn #pos_fn_ident(&self) -> usize { #pos_body }
        });

        prev_fn = Some(pos_fn_ident.clone());
        acc = Some(this_len);
        let pos_fn = pos_fn_ident;

        let ret = field.sanitized_table_getter_return_type(items);
        let body = field.sanitized_table_getter_body(&pos_fn, items);
        getter_methods.push(quote! {
            pub fn #field_name(&self) -> #ret { #body }
        });

        // For offset fields (and arrays of offsets), also generate a resolved getter.
        if let Some(resolved) = field.sanitized_table_offset_getter() {
            getter_methods.push(resolved);
        }
    }

    (pos_methods, getter_methods)
}

/// Returns (args_type, extra_struct_fields, args_init_stmts, arg_getters)
/// for tables that have `#[read_args(...)]`.
fn build_args_info(
    read_args: &Option<crate::parsing::TableReadArgs>,
) -> (
    TokenStream,      // Args type
    Vec<TokenStream>, // extra struct fields
    Vec<TokenStream>, // init stmts for read_sanitized
    Vec<TokenStream>, // getter methods
) {
    match read_args {
        None => (quote!(()), vec![], vec![], vec![]),
        Some(ra) if ra.args.len() == 1 => {
            let ident = &ra.args[0].ident;
            let typ = &ra.args[0].typ;
            (
                quote!(#typ),
                vec![quote!(#ident: #typ)],
                vec![quote!(let #ident = *args;)],
                vec![quote! {
                    pub fn #ident(&self) -> #typ { self.#ident }
                }],
            )
        }
        Some(ra) => {
            let idents: Vec<_> = ra.args.iter().map(|a| &a.ident).collect();
            let types: Vec<_> = ra.args.iter().map(|a| &a.typ).collect();
            let args_type = quote!((#(#types),*));
            let fields = idents
                .iter()
                .zip(types.iter())
                .map(|(i, t)| quote!(#i: #t))
                .collect();
            let inits = vec![quote!(let (#(#idents),*) = *args;)];
            let getters = idents
                .iter()
                .zip(types.iter())
                .map(|(i, t)| {
                    quote! { pub fn #i(&self) -> #t { self.#i } }
                })
                .collect();
            (args_type, fields, inits, getters)
        }
    }
}

/// Build `Self { ptr, field1, field2, ... }` or `Self { ptr }`.
fn build_read_sanitized_init(
    read_args: &Option<crate::parsing::TableReadArgs>,
    phantom_init: Option<&TokenStream>,
) -> TokenStream {
    match read_args {
        None => quote!(Self { ptr, #phantom_init }),
        Some(ra) => {
            let idents: Vec<_> = ra.args.iter().map(|a| &a.ident).collect();
            quote!(Self { ptr, #phantom_init #(#idents),* })
        }
    }
}

/// Returns true if the given type name refers to a `Record` that will get a
/// sanitized version generated — i.e., it has no lifetime and no struct fields
/// whose own types lack sanitized versions.
fn has_sanitized_record(name: &syn::Ident, items: &Items) -> bool {
    match items.get(name) {
        Some(Item::Record(r)) => {
            if r.lifetime.is_some() {
                return false;
            }
            !r.fields.iter().any(|f| {
                matches!(&f.typ, FieldType::Struct { typ } if !has_sanitized_record(typ, items))
            })
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Per-field code generation methods
// ---------------------------------------------------------------------------

impl Field {
    /// Byte-size expression for this field in the sanitized context, or `None`
    /// if the size cannot be statically computed (which poisons all subsequent
    /// `_pos()` methods).
    pub(crate) fn sanitized_byte_len(&self) -> Option<TokenStream> {
        Some(self.computed_len_expr_for_sanitize().unwrap_or_else(|| {
            let typ = self.typ.cooked_type_tokens();
            quote!( #typ::RAW_BYTE_LEN )
        }))
    }

    fn computed_len_expr_for_sanitize(&self) -> Option<TokenStream> {
        if !self.has_computed_len() {
            return None;
        }

        let read_args = self
            .attrs
            .read_with_args
            .as_deref()
            .map(FieldReadArgs::to_tokens_for_table_getter);

        if let FieldType::Struct { typ } = &self.typ {
            return Some(quote!( <#typ as ComputeSize>::compute_size(&#read_args).unwrap_or(0) ));
        }

        Some(match self.attrs.count.as_deref() {
            Some(Count::All(_)) => todo!(),
            Some(other) => {
                let count_expr = other.sanitized_count_expr();
                let size_expr = match &self.typ {
                    FieldType::Array { inner_typ } => {
                        let inner_typ = inner_typ.cooked_type_tokens();
                        quote!( #inner_typ::RAW_BYTE_LEN )
                    }
                    FieldType::ComputedArray(array) => {
                        let inner = array.raw_inner_type();
                        quote!( <#inner as ComputeSize>::compute_size(&#read_args).unwrap_or(0) )
                    }
                    FieldType::VarLenArray(_) => {
                        quote!(compile_error!("sanitize not implemented for VarLenArray"))
                    }
                    _ => unreachable!("count not valid here"),
                };
                if other.is_lit_1() {
                    size_expr
                } else {
                    quote!( (#count_expr).saturating_mul(#size_expr) )
                }
            }
            None => quote!(compile_error!("missing count attribute")),
        })
    }

    // --- Table context ---

    /// Return type of this field's primary getter method in a table.
    pub(crate) fn sanitized_table_getter_return_type(&self, items: &Items) -> TokenStream {
        match &self.typ {
            FieldType::Scalar { typ } => quote!(#typ),
            FieldType::Offset { typ, .. } => quote!(#typ),
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Scalar { typ } => quote!(&'a [BigEndian<#typ>]),
                FieldType::Struct { typ } if has_sanitized_record(typ, items) => {
                    let st = format_ident!("{}Sanitized", typ);
                    quote!(&'a [#st])
                }
                FieldType::Offset { typ, .. } => {
                    if self.attrs.nullable.is_some() {
                        quote!(&'a [BigEndian<Nullable<#typ>>])
                    } else {
                        quote!(&'a [BigEndian<#typ>])
                    }
                }
                _ => quote!(()),
            },
            FieldType::ComputedArray(array) => {
                let inner = array.raw_inner_type();
                let st = format_ident!("{}Sanitized", inner);
                quote!(ComputedArraySanitized<'a, #st <'a>>)
            }
            FieldType::Struct { typ } => {
                let st = format_ident!("{}Sanitized", typ);
                quote!(#st<'a>)
            }
            _ => quote!(()),
        }
    }

    /// Body of this field's primary getter method in a table.
    pub(crate) fn sanitized_table_getter_body(
        &self,
        pos_fn: &syn::Ident,
        items: &Items,
    ) -> TokenStream {
        let read_args = self
            .attrs
            .read_with_args
            .as_deref()
            .map(FieldReadArgs::to_tokens_for_table_getter)
            .unwrap_or_else(|| quote!(()));

        match &self.typ {
            FieldType::Scalar { .. } | FieldType::Offset { .. } => {
                quote!(unsafe { self.ptr.read_at(self.#pos_fn()) })
            }
            FieldType::Array { inner_typ } => {
                // #[count] always present on arrays, would crash before sanitize
                let count_expr = self.attrs.count.as_deref().unwrap().sanitized_count_expr();
                match inner_typ.as_ref() {
                    FieldType::Scalar { .. } | FieldType::Offset { .. } => {
                        quote!(unsafe { self.ptr.read_array_at(self.#pos_fn(), #count_expr) })
                    }
                    FieldType::Struct { typ } if has_sanitized_record(typ, items) => {
                        quote!(unsafe { self.ptr.read_array_at(self.#pos_fn(), #count_expr) })
                    }
                    _ => quote!(compile_error!("not a valid type")),
                }
            }
            FieldType::Struct { .. } => {
                quote! {
                    let ptr = unsafe { self.ptr.for_offset(self.#pos_fn()) };
                    unsafe { ReadSanitized::read_sanitized(ptr, &#read_args) }
                }
            }
            FieldType::ComputedArray(array) => {
                let count_expr = self.attrs.count.as_deref().unwrap().sanitized_count_expr();
                let read_args = self
                    .attrs
                    .read_with_args
                    .as_deref()
                    .map(FieldReadArgs::to_tokens_for_table_getter)
                    .unwrap_or_else(|| quote!(()));
                let inner = array.raw_inner_type();
                quote! {
                    let count = #count_expr;
                    let args = #read_args;
                    let item_len = <#inner as ComputeSize>::compute_size(&args).unwrap_or(0);
                    ComputedArraySanitized::new(
                        unsafe { self.ptr.for_offset(self.#pos_fn()) },
                        count,
                        item_len,
                        args,
                    )
                }
            }
            FieldType::VarLenArray(_) => {
                quote!(unimplemented!(
                    "var-len array not yet supported in read_sanitized"
                ))
            }
            FieldType::PendingResolution { .. } => {
                panic!("unresolved field type in sanitized_table_getter_body")
            }
        }
    }

    /// Resolved offset getter method for a table, if applicable.
    ///
    /// For single `Offset` fields: generates a resolved getter returning the target type.
    /// For array-of-offsets fields: generates an `ArrayOfSanitizedOffsets` getter at
    /// `offset_getter_name` (the "friendly" name like `lookups`), since the primary raw
    /// getter already uses the field name (like `lookup_offsets`).
    pub(crate) fn sanitized_table_offset_getter(&self) -> Option<TokenStream> {
        // Array-of-offsets: emit ArrayOfSanitizedOffsets getter at offset_getter_name.
        if let FieldType::Array { inner_typ } = &self.typ {
            if let FieldType::Offset {
                typ: offset_typ,
                target: OffsetTarget::Table(target_name),
            } = inner_typ.as_ref()
            {
                let getter_name = self.offset_getter_name()?;
                let field_name = &self.name;
                let array_type = if self.attrs.nullable.is_some() {
                    quote!(ArrayOfSanitizedNullableOffsets)
                } else {
                    quote!(ArrayOfSanitizedOffsets)
                };
                let args_expr = self.sanitized_offset_args_expr();
                let (target_type, where_clause) = if target_name == "T" {
                    (
                        quote!(T),
                        Some(quote!(where T: ReadSanitized<'a, Args = ()>)),
                    )
                } else {
                    let st = format_ident!("{}Sanitized", target_name);
                    (quote!(#st<'a>), None)
                };
                let return_type = quote!(#array_type<'a, #target_type, #offset_typ>);
                return Some(quote! {
                    pub fn #getter_name(&self) -> #return_type #where_clause {
                        let offsets = self.#field_name();
                        #array_type::new(offsets, self.ptr, #args_expr)
                    }
                });
            }
        }

        let FieldType::Offset { target, .. } = &self.typ else {
            return None;
        };
        let getter_name = self.offset_getter_name().unwrap();
        let field_name = &self.name;
        let is_nullable = self.attrs.nullable.is_some() || self.attrs.conditional.is_some();

        Some(match target {
            OffsetTarget::Table(target_name) => {
                let mut return_type = if target_name == "T" {
                    target_name.to_token_stream()
                } else {
                    let ident = format_ident!("{}Sanitized", target_name);
                    quote!(#ident<'a>)
                };
                if is_nullable {
                    return_type = quote!(Option<#return_type>);
                }
                let where_clause = (target_name == "T")
                    .then(|| quote!(where T: ReadSanitized<'a, Args = ()> + Default));
                let args_expr = self.sanitized_offset_args_expr();
                let unwrap_or_default = (!is_nullable).then(|| quote!(.unwrap_or_default()));
                quote! {
                    pub fn #getter_name(&self) -> #return_type #where_clause {
                        unsafe { self.#field_name().resolve_sanitized(self.ptr, &#args_expr) #unwrap_or_default }
                    }
                }
            }
            OffsetTarget::Array(_) => {
                quote! {
                    pub fn #getter_name(&self) {
                        unimplemented!("offset to array not yet supported in read_sanitized")
                    }
                }
            }
        })
    }

    // --- Record context ---

    /// Struct field declaration for a sanitized record.
    pub(crate) fn sanitized_record_field_decl(&self) -> TokenStream {
        let name = &self.name;
        let docs = &self.attrs.docs;
        match &self.typ {
            FieldType::Scalar { typ } => quote! { #( #docs )* pub #name: BigEndian<#typ> },
            FieldType::Offset { typ, .. } => quote! { #( #docs )* pub #name: BigEndian<#typ> },
            FieldType::Struct { typ } => {
                let st = format_ident!("{}Sanitized", typ);
                quote! { #( #docs )* pub #name: #st }
            }
            _ => unreachable!("sanitized_record_field_decl called on unsupported field type"),
        }
    }

    /// Getter method for a sanitized record field.
    pub(crate) fn sanitized_record_getter(&self) -> TokenStream {
        let name = &self.name;
        let docs = &self.attrs.docs;
        match &self.typ {
            FieldType::Scalar { typ } => quote! {
                #( #docs )*
                pub fn #name(&self) -> #typ { self.#name.get() }
            },
            FieldType::Offset { typ, .. } => quote! {
                #( #docs )*
                pub fn #name(&self) -> #typ { self.#name.get() }
            },
            FieldType::Struct { typ } => {
                let st = format_ident!("{}Sanitized", typ);
                quote! {
                    #( #docs )*
                    pub fn #name(&self) -> #st { self.#name }
                }
            }
            _ => unreachable!("sanitized_record_getter called on unsupported field type"),
        }
    }

    /// Offset-resolution method for a sanitized record field, if applicable.
    ///
    /// Returns `Some` only for `Offset` fields that have an `#[offset_getter]`;
    /// all other field types return `None`.
    pub(crate) fn sanitized_record_offset_method(&self, _items: &Items) -> Option<TokenStream> {
        if self.attrs.offset_getter.is_some() {
            return None;
        }
        let FieldType::Offset { target, .. } = &self.typ else {
            return None;
        };
        let getter_name = self.offset_getter_name()?;
        let field_name = &self.name;
        let is_nullable = self.attrs.nullable.is_some() || self.attrs.conditional.is_some();
        let args_expr = self.sanitized_offset_args_expr();

        Some(match target {
            OffsetTarget::Table(target_name) => {
                let st = format_ident!("{}Sanitized", target_name);
                if is_nullable {
                    quote! {
                        pub fn #getter_name<'a>(&self, parent_ptr: FontPtr<'a>) -> Option<#st<'a>> {
                            let offset = self.#field_name();
                            unsafe { offset.resolve_sanitized(parent_ptr, &#args_expr) }
                        }
                    }
                } else {
                    quote! {
                        pub fn #getter_name<'a>(&self, parent_ptr: FontPtr<'a>) -> #st<'a> {
                            let offset = self.#field_name();
                            unsafe {
                                offset
                                    .resolve_sanitized(parent_ptr, &#args_expr)
                                    .unwrap_or_default()
                            }
                        }
                    }
                }
            }
            OffsetTarget::Array(_) => {
                quote! {
                    pub fn #getter_name<'a>(&self, _parent_ptr: FontPtr<'a>) {
                        unimplemented!("offset to array not yet supported in read_sanitized records")
                    }
                }
            }
        })
    }

    // --- Private helpers ---

    /// Args expression for `resolve_sanitized`, derived from `#[read_offset_with(...)]`.
    fn sanitized_offset_args_expr(&self) -> TokenStream {
        match self.attrs.read_offset_args.as_deref() {
            None => quote!(()),
            Some(args) if args.inputs.len() == 1 => {
                let f = &args.inputs[0];
                quote!(self.#f())
            }
            Some(args) => {
                let inputs = &args.inputs;
                quote!((#(self.#inputs()),*))
            }
        }
    }
}
