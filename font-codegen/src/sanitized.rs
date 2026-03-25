//! Code generation for `ReadSanitized` types.
//!
//! These are high-efficiency types for reading pre-validated font data without
//! bounds checking, using raw pointer arithmetic via `FontPtr`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::parsing::{
    Field, FieldType, GenericGroup, Item, Items, OffsetTarget, Record, Table, TableFormat,
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

    // Build position and getter methods by walking fields in order.
    let mut pos_methods: Vec<TokenStream> = Vec::new();
    let mut getter_methods: Vec<TokenStream> = Vec::new();

    // State for chaining _pos() methods.
    //
    // We use an enum-like representation:
    //   - `pos_state = Broken`: a previous field had an unknowable size; all
    //     subsequent _pos() methods emit `unimplemented!()`.
    //   - `pos_state = Known { prev_fn, acc }`: `prev_fn` is the last emitted
    //     _pos() method ident, `acc` is the total byte count since that point.
    //     `prev_fn = None, acc = None` means "start of table, offset 0".
    enum PosState {
        Known {
            prev_fn: Option<syn::Ident>,
            acc: Option<TokenStream>, // None means 0
        },
        Broken,
    }
    let mut pos_state = PosState::Known {
        prev_fn: None,
        acc: None,
    };

    for field in item.fields.iter() {
        let this_len = field.sanitized_byte_len(items);

        if field.attrs.skip_getter.is_some() {
            // Don't generate getter/pos, but account for this field's bytes.
            if let PosState::Known { ref mut acc, .. } = pos_state {
                match (acc.take(), this_len) {
                    (None, Some(len)) => *acc = Some(len),
                    (Some(prev), Some(len)) => *acc = Some(quote!(#prev + #len)),
                    // If this_len is None, position tracking is broken.
                    (_, None) => pos_state = PosState::Broken,
                }
            }
            continue;
        }

        let field_name = &field.name;
        let pos_fn = format_ident!("{}_pos", field_name);

        // _pos() method
        let pos_body = match &pos_state {
            PosState::Broken => {
                quote! { unimplemented!("position cannot be computed: a prior field has unknown size") }
            }
            PosState::Known {
                prev_fn: None,
                acc: None,
            } => quote!(0),
            PosState::Known {
                prev_fn: None,
                acc: Some(offset),
            } => quote! ( #offset ),
            PosState::Known {
                prev_fn: Some(prev_fn),
                acc: None,
            } => quote! ( self.#prev_fn() ),
            PosState::Known {
                prev_fn: Some(prev_fn),
                acc: Some(acc),
            } => quote! (  self.#prev_fn() + #acc ),
        };
        pos_methods.push(quote! {
            fn #pos_fn(&self) -> usize { #pos_body }
        });

        // Advance state for the next field.
        match this_len {
            Some(len) => {
                pos_state = PosState::Known {
                    prev_fn: Some(pos_fn.clone()),
                    acc: Some(len),
                };
            }
            None => {
                // This field's size is unknown; all following fields are broken.
                pos_state = PosState::Broken;
            }
        }

        // Generate the primary getter method.
        let getter_name = field.sanitized_table_getter_name();
        let ret = field.sanitized_table_getter_return_type(items);
        let body = field.sanitized_table_getter_body(&pos_fn, items);
        getter_methods.push(quote! {
            pub fn #getter_name(&self) -> #ret { #body }
        });

        // For offset fields, also generate a resolved getter.
        if let Some(resolved) = field.sanitized_table_resolved_getter(items) {
            getter_methods.push(resolved);
        }
    }

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

/// Generate a `ReadSanitized`-style struct for a record type.
///
/// Only zerocopy-compatible records (no lifetimes, all scalar/offset/struct fields)
/// get a sanitized version. Records with arrays are skipped.
pub(crate) fn generate_read_sanitized_record(
    item: &Record,
    items: &Items,
) -> syn::Result<TokenStream> {
    // Skip records with lifetimes (they contain arrays) — complex, handle later.
    if item.lifetime.is_some() {
        return Ok(Default::default());
    }

    // Skip records that contain embedded struct fields whose type doesn't have
    // a sanitized version (e.g. extern types, variable-size records).
    let has_unsupported_struct = item
        .fields
        .iter()
        .any(|f| matches!(&f.typ, FieldType::Struct { typ } if !has_sanitized_record(typ, items)));
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

    // Skip if any active variant's concrete type lacks a sanitized version.
    if active_variants
        .iter()
        .any(|v| !has_sanitized_table(v.type_name(), items))
    {
        return Ok(Default::default());
    }

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

    Ok(quote! {
        #[derive(Clone)]
        pub enum #sanitized_name<'a> {
            #( #variant_defs ),*
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

/// Generate a stub for a generic-group table.
pub(crate) fn generate_read_sanitized_group(_item: &GenericGroup) -> syn::Result<TokenStream> {
    Ok(Default::default())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Returns true if the given type name refers to a regular `Table` in the item set
/// (not a TableFormat, GenericGroup, Record, or external type).
fn has_sanitized_table(name: &syn::Ident, items: &Items) -> bool {
    matches!(items.get(name), Some(Item::Table(t)) if t.attrs.write_only.is_none())
}

/// Returns true if the given type name refers to a format group that will have
/// a sanitized enum generated (i.e. all active variants have sanitized tables).
fn has_sanitized_format_group(name: &syn::Ident, items: &Items) -> bool {
    match items.get(name) {
        Some(Item::Format(tf)) => tf
            .variants
            .iter()
            .filter(|v| v.attrs.write_only.is_none())
            .all(|v| has_sanitized_table(v.type_name(), items)),
        _ => false,
    }
}

/// Returns true if the given type name has a sanitized version — either a
/// concrete table or a format-dispatch enum.
fn has_sanitized_version(name: &syn::Ident, items: &Items) -> bool {
    has_sanitized_table(name, items) || has_sanitized_format_group(name, items)
}

/// Returns true if the target table requires non-trivial `Args` (i.e. has `#[read_args(...)]`).
fn table_needs_args(name: &syn::Ident, items: &Items) -> bool {
    matches!(items.get(name), Some(Item::Table(t)) if t.fields.read_args.is_some())
}

/// Returns true if the given type name refers to a zerocopy-compatible `Record`
/// that will get a sanitized version generated.
fn has_sanitized_record(name: &syn::Ident, items: &Items) -> bool {
    match items.get(name) {
        Some(Item::Record(r)) => r.lifetime.is_none(),
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
    pub(crate) fn sanitized_byte_len(&self, items: &Items) -> Option<TokenStream> {
        match &self.typ {
            FieldType::Scalar { typ } => Some(quote!(#typ::RAW_BYTE_LEN)),
            FieldType::Struct { typ } => {
                if has_sanitized_record(typ, items) {
                    Some(quote!(#typ::RAW_BYTE_LEN))
                } else {
                    None
                }
            }
            FieldType::Offset { typ, .. } => Some(quote!(#typ::RAW_BYTE_LEN)),
            FieldType::Array { inner_typ } => {
                let inner_size = match inner_typ.as_ref() {
                    FieldType::Scalar { typ } => quote!(#typ::RAW_BYTE_LEN),
                    FieldType::Struct { typ } => {
                        if has_sanitized_record(typ, items) {
                            quote!(#typ::RAW_BYTE_LEN)
                        } else {
                            quote!(compile_error!("fancy struct in array shouldn't happen"))
                        }
                    }
                    FieldType::Offset { typ, .. } => quote!(#typ::RAW_BYTE_LEN),
                    _ => quote!(compile_error!("unknown type in array")),
                };
                let count_expr = self
                    .attrs
                    .count
                    .as_deref()
                    .and_then(|c| c.sanitized_count_expr())?;
                Some(quote!((#count_expr).saturating_mul(#inner_size)))
            }
            FieldType::ComputedArray(_) | FieldType::VarLenArray(_) => None,
            FieldType::PendingResolution { .. } => {
                panic!("unresolved type in sanitized_byte_len")
            }
        }
    }

    // --- Table context ---

    /// Name of this field's primary getter method in a table.
    ///
    /// For array-of-offsets fields the getter is named by `#[offset_getter]`,
    /// not by the field itself. For all other types the field name is used.
    pub(crate) fn sanitized_table_getter_name(&self) -> syn::Ident {
        match &self.typ {
            FieldType::Array { inner_typ }
                if matches!(inner_typ.as_ref(), FieldType::Offset { .. }) =>
            {
                self.offset_getter_name().unwrap()
            }
            _ => self.name.clone(),
        }
    }

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
                FieldType::Offset { typ, .. } => quote!(&'a [BigEndian<#typ>]),
                _ => quote!(()),
            },
            FieldType::Struct { typ } if has_sanitized_record(typ, items) => {
                let st = format_ident!("{}Sanitized", typ);
                quote!(#st)
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
        match &self.typ {
            FieldType::Scalar { .. } | FieldType::Offset { .. } => {
                quote!(unsafe { self.ptr.read_at(self.#pos_fn()) })
            }
            FieldType::Array { inner_typ } => {
                let count_expr = self.sanitized_count_expr();
                match inner_typ.as_ref() {
                    FieldType::Scalar { .. } | FieldType::Offset { .. } => {
                        quote!(unsafe { self.ptr.read_array_at(self.#pos_fn(), #count_expr) })
                    }
                    FieldType::Struct { typ } if has_sanitized_record(typ, items) => {
                        quote!(unsafe { self.ptr.read_array_at(self.#pos_fn(), #count_expr) })
                    }
                    _ => quote!(unimplemented!("record type lacks a ReadSanitized impl")),
                }
            }
            FieldType::Struct { typ } if has_sanitized_record(typ, items) => {
                quote!(unimplemented!("struct field"))
            }
            FieldType::Struct { .. } => {
                quote!(unimplemented!("struct type lacks a ReadSanitized impl"))
            }
            FieldType::ComputedArray(_) | FieldType::VarLenArray(_) => {
                quote!(unimplemented!(
                    "computed/var-len array not yet supported in read_sanitized"
                ))
            }
            FieldType::PendingResolution { .. } => {
                panic!("unresolved field type in sanitized_table_getter_body")
            }
        }
    }

    /// Resolved offset getter method for a table, if applicable.
    ///
    /// Returns `Some` only for `Offset` fields; all other field types return `None`.
    pub(crate) fn sanitized_table_resolved_getter(&self, items: &Items) -> Option<TokenStream> {
        let FieldType::Offset { target, .. } = &self.typ else {
            return None;
        };
        let getter_name = self.offset_getter_name().unwrap();
        let field_name = &self.name;
        let is_nullable = self.attrs.nullable.is_some() || self.attrs.conditional.is_some();

        Some(match target {
            OffsetTarget::Table(target_name) => {
                if has_sanitized_version(target_name, items) {
                    let st = format_ident!("{}Sanitized", target_name);
                    if table_needs_args(target_name, items) && self.attrs.read_offset_args.is_none()
                    {
                        quote! {
                            pub fn #getter_name(&self) {
                                unimplemented!("target requires args not available from this field")
                            }
                        }
                    } else {
                        let args_expr = self.sanitized_offset_args_expr();
                        if is_nullable {
                            quote! {
                                pub fn #getter_name(&self) -> Option<#st<'a>> {
                                    unsafe { self.#field_name().resolve_sanitized(self.ptr.clone(), &#args_expr) }
                                }
                            }
                        } else {
                            quote! {
                                pub fn #getter_name(&self) -> #st<'a> {
                                    unsafe {
                                        self.#field_name()
                                            .resolve_sanitized(self.ptr.clone(), &#args_expr)
                                            .unwrap_or_default()
                                    }
                                }
                            }
                        }
                    }
                } else {
                    quote! {
                        pub fn #getter_name(&self) {
                            unimplemented!("target type lacks a ReadSanitized impl")
                        }
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
    pub(crate) fn sanitized_record_offset_method(&self, items: &Items) -> Option<TokenStream> {
        let FieldType::Offset { target, .. } = &self.typ else {
            return None;
        };
        let getter_name = self.offset_getter_name()?;
        let field_name = &self.name;
        let is_nullable = self.attrs.nullable.is_some() || self.attrs.conditional.is_some();
        let args_expr = self.sanitized_offset_args_expr();

        Some(match target {
            OffsetTarget::Table(target_name) => {
                if has_sanitized_version(target_name, items) {
                    let st = format_ident!("{}Sanitized", target_name);
                    if table_needs_args(target_name, items) && self.attrs.read_offset_args.is_none()
                    {
                        quote! {
                            pub fn #getter_name<'a>(&self, _parent_ptr: FontPtr<'a>) {
                                unimplemented!("target requires args not available from this field")
                            }
                        }
                    } else if is_nullable {
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
                } else {
                    quote! {
                        pub fn #getter_name<'a>(&self, _parent_ptr: FontPtr<'a>) {
                            unimplemented!("target type lacks a ReadSanitized impl")
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

    /// Count expression for an array field: `self.count_field() as _`, a literal, or
    /// `unimplemented!()` for complex counts.
    fn sanitized_count_expr(&self) -> TokenStream {
        self.attrs
            .count
            .as_deref()
            .unwrap()
            .sanitized_count_expr()
            .unwrap_or_else(|| {
                quote!(unimplemented!(
                    "'all' count not supported in sanitized context"
                ))
            })
    }

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
