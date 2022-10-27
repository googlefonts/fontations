//! methods on fields

use std::ops::Deref;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::spanned::Spanned;

use crate::parsing::{Attr, FieldValidation, OffsetTarget, Phase};

use super::parsing::{
    Count, CustomCompile, Field, FieldReadArgs, FieldType, Fields, NeededWhen, Record,
    ReferencedFields,
};

impl Fields {
    pub(crate) fn new(mut fields: Vec<Field>) -> syn::Result<Self> {
        let referenced_fields = fields
            .iter()
            .flat_map(Field::input_fields)
            .collect::<ReferencedFields>();

        for field in fields.iter_mut() {
            field.read_at_parse_time =
                field.attrs.version.is_some() || referenced_fields.needs_at_parsetime(&field.name);
        }

        Ok(Fields {
            fields,
            read_args: None,
            referenced_fields,
        })
    }

    pub(crate) fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        let mut custom_offset_data_fld: Option<&Field> = None;
        let mut normal_offset_data_fld = None;
        for (i, fld) in self.fields.iter().enumerate() {
            if let Some(attr) = fld.attrs.offset_data.as_ref() {
                if let Some(prev_field) = custom_offset_data_fld.replace(fld) {
                    if prev_field.attrs.offset_data.as_ref().unwrap().attr != attr.attr {
                        return Err(syn::Error::new(fld.name.span(), format!("field has custom offset data, but previous field '{}' already specified different custom offset data", prev_field.name)));
                    }
                }
            } else if fld.is_offset_or_array_of_offsets() {
                normal_offset_data_fld = Some(fld);
            }

            if (matches!(fld.typ, FieldType::VarLenArray(_))
                || matches!(fld.attrs.count.as_deref(), Some(Count::All)))
                && i != self.fields.len() - 1
            {
                return Err(syn::Error::new(
                    fld.name.span(),
                    "#[count(..)] or VarLenArray fields can only be last field in table.",
                ));
            }
            fld.sanity_check(phase)?;
        }

        if let (Some(custom), Some(normal)) = (custom_offset_data_fld, normal_offset_data_fld) {
            return Err(syn::Error::new(custom.name.span(), format!("Not implemented: field requires custom offset data, but sibling field '{}' expects default offset data", normal.name)));
        }
        Ok(())
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }

    pub(crate) fn iter_compile_decls(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().filter_map(Field::compile_field_decl)
    }

    pub(crate) fn iter_compile_write_stmts(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().map(Field::compile_write_stmt)
    }

    pub(crate) fn iter_compile_default_inits(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.fields.iter().filter_map(Field::compile_default_init)
    }

    /// `Ok(true)` if no fields have custom default values, `Ok(false)` otherwise.
    ///
    /// This serves double duty as a validation method: if we know that default
    /// should not be derived on a field (such as with version fields) but there
    /// is no apprporiate annotation, we will return an error explaining the problem.
    /// This is more helpful than generating code that does not compile, or compile_write_stmt
    /// but is likely not desired.
    pub(crate) fn can_derive_default(&self) -> syn::Result<bool> {
        for field in self.iter() {
            if !field.supports_derive_default()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    // we use this to add disable a clippy lint if needed
    pub(crate) fn compile_write_contains_int_casts(&self) -> bool {
        self.iter().any(Field::compile_write_contains_int_cast)
    }

    /// If this table has a version field, return it.
    pub(crate) fn version_field(&self) -> Option<&Field> {
        self.iter()
            .find_map(|fld| fld.attrs.version.is_some().then_some(fld))
    }

    // used for validating lengths. handles both fields and 'virtual fields',
    // e.g. arguments passed in FontReadWithArgs
    fn get_scalar_field_type(&self, name: &syn::Ident) -> &syn::Ident {
        self.iter()
            .find(|fld| &fld.name == name)
            .map(|fld| match &fld.typ {
                FieldType::Scalar { typ } => typ,
                _ => panic!("not a scalar field"),
            })
            .or_else(|| {
                self.read_args.as_ref().and_then(|args| {
                    args.args
                        .iter()
                        .find(|arg| &arg.ident == name)
                        .map(|arg| &arg.typ)
                })
            })
            .expect("validate that count references existing fields")
    }

    pub(crate) fn compilation_validation_stmts(&self) -> Vec<TokenStream> {
        let mut stmts = Vec::new();
        for field in self.fields.iter() {
            if field.is_computed() {
                continue;
            }
            let name = &field.name;
            let name_str = field.name.to_string();
            let validation_call = match field.attrs.validation.as_deref() {
                Some(FieldValidation::Skip) => continue,
                Some(FieldValidation::Custom(ident)) => Some(quote!( self.#ident(ctx); )),
                None if field.gets_recursive_validation() => {
                    Some(quote!( self.#name.validate_impl(ctx); ))
                }
                None => None,
            };

            let is_single_nullable_offset = field.is_nullable() && !field.is_array();
            let required_by_version = field
                .attrs
                .available
                .as_ref()
                .filter(|_| !is_single_nullable_offset)
                .map(|attr| {
                    let available = &attr.attr;
                    quote! {
                        if version.compatible(#available) && self.#name.is_none() {
                            ctx.report(format!("field must be present for version {version}"));
                        }
                    }
                });

            // this all deals with the case where we have an optional field.
            let maybe_check_is_some = required_by_version
                .is_some()
                .then(|| quote!(self.#name.is_some() &&));
            let maybe_unwrap = required_by_version
                .is_some()
                .then(|| quote!(.as_ref().unwrap()));

            let array_len_check = if let Some(Count::Field(count_name)) =
                field.attrs.count.as_deref()
            {
                let typ = self.get_scalar_field_type(count_name);
                Some(quote! {
                    if #maybe_check_is_some self.#name #maybe_unwrap.len() > (#typ::MAX as usize) {
                        ctx.report("array excedes max length");
                    }
                })
            } else {
                None
            };

            if validation_call.is_some()
                || array_len_check.is_some()
                || required_by_version.is_some()
            {
                stmts.push(quote! {
                    ctx.in_field(#name_str, |ctx| {
                        #required_by_version
                        #array_len_check
                        #validation_call
                    });
                })
            }
            //TODO: also add a custom validation statements
        }
        stmts
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_obj_requires_offset_data(&self, in_record: bool) -> bool {
        self.iter()
            .any(|fld| fld.from_obj_requires_offset_data(in_record))
    }

    pub(crate) fn iter_from_obj_ref_stmts(
        &self,
        in_record: bool,
    ) -> impl Iterator<Item = TokenStream> + '_ {
        self.iter()
            .flat_map(move |fld| fld.from_obj_ref_stmt(in_record))
    }

    pub(crate) fn iter_field_traversal_match_arms(
        &self,
        in_record: bool,
    ) -> impl Iterator<Item = TokenStream> + '_ {
        let pass_data = in_record.then(|| quote!(_data));
        self.fields
            .iter()
            .filter(|fld| fld.has_getter())
            .enumerate()
            .map(move |(i, fld)| {
                let condition = fld
                    .attrs
                    .available
                    .as_ref()
                    .map(|v| quote!(if version.compatible(#v)));
                let rhs = traversal_arm_for_field(fld, in_record, pass_data.as_ref());
                quote!( #i #condition => Some(#rhs) )
            })
    }
}

fn big_endian(typ: &syn::Ident) -> TokenStream {
    if typ == "u8" {
        return quote!(#typ);
    }
    quote!(BigEndian<#typ>)
}

fn traversal_arm_for_field(
    fld: &Field,
    in_record: bool,
    pass_data: Option<&TokenStream>,
) -> TokenStream {
    let name_str = &fld.name.to_string();
    let name = &fld.name;
    let maybe_unwrap = fld.attrs.available.is_some().then(|| quote!(.unwrap()));
    if let Some(traverse_with) = &fld.attrs.traverse_with {
        let traverse_fn = &traverse_with.attr;
        return quote!(Field::new(#name_str, self.#traverse_fn(#pass_data)));
    }
    match &fld.typ {
        FieldType::Offset {
            target: Some(OffsetTarget::Array(inner)),
            ..
        } if matches!(inner.deref(), FieldType::Struct { .. }) => {
            let typ = inner.cooked_type_tokens();
            let getter = fld.offset_getter_name();
            let offset_data = fld.offset_getter_data_src();
            quote!(Field::new(
                    #name_str,
                    traversal::FieldType::offset_to_array_of_records(
                        self.#name()#maybe_unwrap,
                        self.#getter(#pass_data)#maybe_unwrap,
                        stringify!(#typ),
                        #offset_data,
                    )
            ))
        }
        FieldType::Offset {
            target: Some(target),
            ..
        } => {
            let constructor_name = match target {
                OffsetTarget::Table(_) => quote!(offset),
                OffsetTarget::Array(_) => quote!(offset_to_array_of_scalars),
            };
            let getter = fld.offset_getter_name();
            quote!(Field::new(#name_str, FieldType::#constructor_name(self.#name()#maybe_unwrap, self.#getter(#pass_data)#maybe_unwrap)))
        }
        FieldType::Offset { .. } => {
            quote!(Field::new(#name_str, FieldType::unknown_offset(self.#name()#maybe_unwrap)))
        }
        FieldType::Scalar { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),

        FieldType::Array { inner_typ } => match inner_typ.as_ref() {
            FieldType::Scalar { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),
            //HACK: glyf has fields that are [u8]
            FieldType::Struct { typ } if typ == "u8" => {
                quote!(Field::new( #name_str, self.#name()#maybe_unwrap))
            }
            FieldType::Struct { typ } if !in_record => {
                let offset_data = fld.offset_getter_data_src();
                quote!(Field::new(
                        #name_str,
                        traversal::FieldType::array_of_records(
                            stringify!(#typ),
                            self.#name()#maybe_unwrap,
                            #offset_data,
                        )
                ))
            }

            FieldType::Offset {
                target: Some(OffsetTarget::Table(target)),
                ..
            } => {
                let maybe_data = pass_data.is_none().then(|| quote!(let data = self.data;));
                let args_if_needed = fld.attrs.read_offset_args.as_ref().map(|args| {
                    let args = args.to_tokens_for_table_getter();
                    quote!(let args = #args;)
                });
                let resolve = match fld.attrs.read_offset_args.as_deref() {
                    None => quote!(resolve::<#target>(data)),
                    Some(_) => quote!(resolve_with_args::<#target>(data, &args)),
                };

                quote! {{
                    #maybe_data
                    #args_if_needed
                    Field::new(#name_str,
                        FieldType::array_of_offsets(
                            better_type_name::<#target>(),
                            self.#name()#maybe_unwrap,
                            move |off| {
                                let target = off.get().#resolve;
                                FieldType::offset(off.get(), target)
                            }
                        ))
                }}
            }
            FieldType::Offset {
                target: Some(OffsetTarget::Array(_)),
                ..
            } => panic!(
                "achievement unlocked: 'added arrays of offsets to arrays to OpenType spec' {:#?}",
                fld
            ),
            FieldType::Offset { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),
            _ => quote!(compile_error!("unhandled traversal case")),
        },
        FieldType::ComputedArray(arr) => {
            // if we are in a record, we pass in empty data. This lets us
            // avoid a special case in the single instance where this happens, in
            // Class1Record
            let data = in_record
                .then(|| quote!(FontData::new(&[])))
                .unwrap_or_else(|| fld.offset_getter_data_src());
            // in a record we return things by value, so clone
            let maybe_clone = in_record.then(|| quote!(.clone()));
            let typ_str = arr.raw_inner_type().to_string();
            quote!(Field::new(
                    #name_str,
                    traversal::FieldType::computed_array(
                        #typ_str,
                        self.#name()#maybe_clone #maybe_unwrap,
                        #data
                    )
            ))
        }
        FieldType::VarLenArray(_) => {
            quote!(Field::new(#name_str, traversal::FieldType::var_array(self.#name()#maybe_unwrap)))
        }
        // HACK: who wouldn't want to hard-code ValueRecord handling
        FieldType::Struct { typ } if typ == "ValueRecord" => {
            let clone = in_record.then(|| quote!(.clone()));
            quote!(Field::new(#name_str, self.#name() #clone #maybe_unwrap))
        }
        FieldType::Struct { .. } => {
            quote!(compile_error!(concat!("another weird type: ", #name_str)))
        }
        FieldType::PendingResolution { .. } => panic!("Should have resolved {:#?}", fld),
    }
}

fn check_resolution(phase: Phase, field_type: &FieldType) -> syn::Result<()> {
    if let Phase::Parse = phase {
        return Ok(());
    }
    if let FieldType::PendingResolution { typ } = field_type {
        return Err(syn::Error::new(
            typ.span(),
            format!(
                "{}: be ye struct or scalar? - we certainly don't know.",
                typ
            ),
        ));
    }
    Ok(())
}

impl Field {
    pub(crate) fn type_for_record(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ, .. } if self.is_nullable() => {
                quote!(BigEndian<Nullable<#typ>>)
            }
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => big_endian(typ),
            FieldType::Struct { typ } => typ.to_token_stream(),
            FieldType::ComputedArray(array) => {
                let inner = array.type_with_lifetime();
                quote!(ComputedArray<'a, #inner>)
            }
            FieldType::VarLenArray(_) => quote!(compile_error("VarLenArray not used in records?")),
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ, .. } if self.is_nullable() => {
                    quote!(&'a [BigEndian<Nullable<#typ>>])
                }
                FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
                    let be = big_endian(typ);
                    quote!(&'a [#be])
                }
                FieldType::Struct { typ } => quote!( &[#typ] ),
                FieldType::PendingResolution { typ } => {
                    panic!("Should have resolved {}", quote! { #typ })
                }
                _ => unreachable!("no nested arrays"),
            },
            FieldType::PendingResolution { typ } => {
                panic!("Should have resolved {}", quote! { #typ })
            }
        }
    }

    pub(crate) fn shape_byte_range_fn_name(&self) -> syn::Ident {
        quote::format_ident!("{}_byte_range", &self.name)
    }

    pub(crate) fn shape_byte_len_field_name(&self) -> syn::Ident {
        quote::format_ident!("{}_byte_len", &self.name)
    }

    pub(crate) fn shape_byte_start_field_name(&self) -> syn::Ident {
        // used when fields are optional
        quote::format_ident!("{}_byte_start", &self.name)
    }

    pub(crate) fn is_array(&self) -> bool {
        matches!(&self.typ, FieldType::Array { .. })
    }

    pub(crate) fn is_computed_array(&self) -> bool {
        matches!(&self.typ, FieldType::ComputedArray { .. })
    }

    fn is_var_array(&self) -> bool {
        matches!(&self.typ, FieldType::VarLenArray { .. })
    }

    pub(crate) fn has_computed_len(&self) -> bool {
        self.attrs.count.is_some() || self.attrs.read_with_args.is_some()
    }

    pub(crate) fn is_version_dependent(&self) -> bool {
        self.attrs.available.is_some()
    }

    /// Sanity check we are in a sane state for the end of phase
    fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        check_resolution(phase, &self.typ)?;
        if let FieldType::Array { inner_typ } = &self.typ {
            if matches!(
                inner_typ.as_ref(),
                FieldType::Array { .. } | FieldType::ComputedArray(_)
            ) {
                return Err(syn::Error::new(
                    self.name.span(),
                    "nested arrays are not allowed",
                ));
            }
            check_resolution(phase, inner_typ)?;
        }
        if let FieldType::Offset {
            target: Some(OffsetTarget::Array(inner_typ)),
            ..
        } = &self.typ
        {
            check_resolution(phase, inner_typ)?;
        }
        if self.is_array() && self.attrs.count.is_none() {
            return Err(syn::Error::new(
                self.name.span(),
                "array requires #[count] attribute",
            ));
        }
        if let Some(args) = &self.attrs.read_with_args {
            match &self.typ {
                FieldType::ComputedArray(array) if self.attrs.count.is_none() => {
                    return Err(syn::Error::new(array.span(), "missing count attribute"));
                }
                FieldType::Offset { .. } => (),
                FieldType::Array { inner_typ, .. }
                    if matches!(inner_typ.as_ref(), FieldType::Offset { .. }) => {}
                FieldType::Scalar { .. } | FieldType::Array { .. } => {
                    return Err(syn::Error::new(
                        args.span(),
                        "attribute not valid on this type",
                    ))
                }
                _ => (),
            }
        }

        Ok(())
    }

    fn is_nullable(&self) -> bool {
        self.attrs.nullable.is_some()
    }

    fn is_computed(&self) -> bool {
        self.attrs.format.is_some() || self.attrs.compile.is_some()
    }

    pub(crate) fn validate_at_parse(&self) -> bool {
        false
        //FIXME: validate fields?
        //self.attrs.format.is_some()
    }

    pub(crate) fn has_getter(&self) -> bool {
        self.attrs.skip_getter.is_none()
    }

    pub(crate) fn shape_len_expr(&self) -> TokenStream {
        // is this a scalar/offset? then it's just 'RAW_BYTE_LEN'
        // is this computed? then it is stored
        match &self.typ {
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
                quote!(#typ::RAW_BYTE_LEN)
            }
            FieldType::Struct { .. }
            | FieldType::Array { .. }
            | FieldType::ComputedArray { .. }
            | FieldType::VarLenArray(_) => {
                let len_field = self.shape_byte_len_field_name();
                let try_op = self.is_version_dependent().then(|| quote!(?));
                quote!(self.#len_field #try_op)
            }
            FieldType::PendingResolution { .. } => panic!("Should have resolved {:?}", self),
        }
    }

    /// iterate the names of fields that are required for parsing or instantiating
    /// this field.
    fn input_fields(&self) -> impl Iterator<Item = (syn::Ident, NeededWhen)> + '_ {
        self.attrs
            .count
            .as_ref()
            .into_iter()
            .flat_map(|count| {
                count
                    .iter_referenced_fields()
                    .cloned()
                    .map(|fld| (fld, NeededWhen::Parse))
            })
            .chain(
                self.attrs
                    .read_with_args
                    .as_ref()
                    .into_iter()
                    .flat_map(|args| args.inputs.iter().map(|x| (x.clone(), NeededWhen::Both))),
            )
            .chain(
                self.attrs
                    .read_offset_args
                    .as_ref()
                    .into_iter()
                    .flat_map(|args| {
                        args.inputs
                            .iter()
                            .cloned()
                            .map(|arg| (arg, NeededWhen::Runtime))
                    }),
            )
    }

    /// 'raw' as in this does not include handling offset resolution
    pub(crate) fn raw_getter_return_type(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ, .. } if self.is_nullable() => quote!(Nullable<#typ>),
            FieldType::Offset { typ, .. }
            | FieldType::Scalar { typ }
            | FieldType::Struct { typ } => typ.to_token_stream(),
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ, .. } if self.is_nullable() => {
                    quote!(&'a [BigEndian<Nullable<#typ>>])
                }
                FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
                    let be = big_endian(typ);
                    quote!(&'a [#be])
                }
                FieldType::Struct { typ } => quote!(&'a [#typ]),
                FieldType::PendingResolution { typ } => quote!( &'a [#typ] ),
                _ => unreachable!("An array should never contain {:#?}", inner_typ),
            },
            FieldType::ComputedArray(array) => {
                let inner = array.type_with_lifetime();
                quote!(ComputedArray<'a, #inner>)
            }
            FieldType::VarLenArray(array) => {
                let inner = array.type_with_lifetime();
                quote!(VarLenArray<'a, #inner>)
            }
            FieldType::PendingResolution { .. } => panic!("Should have resolved {:?}", self),
        }
    }

    pub(crate) fn owned_type(&self) -> TokenStream {
        if let Some(typ) = &self.attrs.compile_type {
            return typ.into_token_stream();
        }
        self.typ
            .compile_type(self.is_nullable(), self.is_version_dependent())
    }

    pub(crate) fn table_getter(&self, generic: Option<&syn::Ident>) -> Option<TokenStream> {
        if !self.has_getter() {
            return None;
        }

        let name = &self.name;
        let is_array = self.is_array();
        let is_var_array = self.is_var_array();
        let is_versioned = self.is_version_dependent();

        let mut return_type = self.raw_getter_return_type();
        if is_versioned {
            return_type = quote!(Option<#return_type>);
        }

        let range_stmt = self.getter_range_stmt();
        let mut read_stmt = if let Some(args) = &self.attrs.read_with_args {
            let get_args = args.to_tokens_for_table_getter();
            quote!( self.data.read_with_args(range, &#get_args).unwrap() )
        } else if is_var_array {
            quote!(VarLenArray::read(self.data.split_off(range.start).unwrap()).unwrap())
        } else if is_array {
            quote!(self.data.read_array(range).unwrap())
        } else {
            quote!(self.data.read_at(range.start).unwrap())
        };
        if is_versioned {
            read_stmt = quote!(Some(#read_stmt));
        }

        let docs = &self.attrs.docs;
        let offset_getter = self.typed_offset_field_getter(generic, None);

        Some(quote! {
            #( #docs )*
            pub fn #name(&self) -> #return_type {
                let range = #range_stmt;
                #read_stmt
            }

            #offset_getter
        })
    }

    pub(crate) fn record_getter(&self, record: &Record) -> Option<TokenStream> {
        if !self.has_getter() {
            return None;
        }
        let name = &self.name;
        let docs = &self.attrs.docs;
        let return_type = self.raw_getter_return_type();
        // records are actually instantiated; their fields exist, so we return
        // them by reference. This differs from tables, which have to instantiate
        // their fields on access.
        let add_borrow_just_for_record = matches!(
            self.typ,
            FieldType::Struct { .. } | FieldType::ComputedArray { .. }
        )
        .then(|| quote!(&));

        let getter_expr = match &self.typ {
            FieldType::Scalar { typ } | FieldType::Offset { typ, .. } => {
                if typ == "u8" {
                    quote!(self.#name)
                } else {
                    quote!(self.#name.get())
                }
            }
            FieldType::Struct { .. }
            | FieldType::ComputedArray { .. }
            | FieldType::VarLenArray(_) => quote!(&self.#name),
            FieldType::Array { .. } => quote!(self.#name),
            FieldType::PendingResolution { .. } => {
                panic!("Should have resolved {:?}", self)
            }
        };

        let offset_getter = self.typed_offset_field_getter(None, Some(record));
        Some(quote! {
            #(#docs)*
            pub fn #name(&self) -> #add_borrow_just_for_record #return_type {
                #getter_expr
            }

            #offset_getter
        })
    }

    fn getter_range_stmt(&self) -> TokenStream {
        let shape_range_fn_name = self.shape_byte_range_fn_name();
        let try_op = self.is_version_dependent().then(|| quote!(?));
        quote!( self.shape.#shape_range_fn_name() #try_op )
    }

    fn typed_offset_field_getter(
        &self,
        generic: Option<&syn::Ident>,
        record: Option<&Record>,
    ) -> Option<TokenStream> {
        let (_, target) = match &self.typ {
            _ if self.attrs.offset_getter.is_some() => return None,
            FieldType::Offset {
                typ,
                target: Some(target),
            } => (typ, target),
            FieldType::Array { inner_typ, .. } => match inner_typ.as_ref() {
                FieldType::Offset {
                    typ,
                    target: Some(target),
                } => (typ, target),
                _ => return None,
            },
            _ => return None,
        };

        let raw_name = &self.name;
        let getter_name = self.offset_getter_name().unwrap();
        let target_is_generic =
            matches!(target, OffsetTarget::Table(ident) if Some(ident) == generic);
        let where_read_clause = target_is_generic.then(|| quote!(where T: FontRead<'a>));
        let mut return_type = target.getter_return_type(target_is_generic);

        if self.is_nullable() || (self.attrs.available.is_some() && !self.is_array()) {
            return_type = quote!(Option<#return_type>);
        }
        if self.is_array() {
            return_type = quote!(impl Iterator<Item=#return_type> + 'a);
            if self.attrs.available.is_some() {
                return_type = quote!(Option<#return_type>);
            }
        }

        let resolve = match self.attrs.read_offset_args.as_deref() {
            None => quote!(resolve(data)),
            Some(_) => quote!(resolve_with_args(data, &args)),
        };

        let args_if_needed = self.attrs.read_offset_args.as_ref().map(|args| {
            let args = args.to_tokens_for_table_getter();
            quote!(let args = #args;)
        });

        // if a record, data is passed in
        let input_data_if_needed = record.is_some().then(|| quote!(, data: FontData<'a>));
        let decl_lifetime_if_needed =
            record.and_then(|x| x.lifetime.is_none().then(|| quote!(<'a>)));

        // if a table, data is self.data, else it is passed as an argument
        let data_alias_if_needed = record.is_none().then(|| quote!(let data = self.data;));

        let docs = format!(" Attempt to resolve [`{raw_name}`][Self::{raw_name}].");
        let (base_method, convert_impl) = if self.is_array() {
            (
                &self.name,
                quote!( .iter().map(move |off| off.get().#resolve) ),
            )
        } else {
            (raw_name, quote!( .#resolve))
        };

        let getter_impl = if self.is_version_dependent() {
            // if this is not an array and *is* nullable we need to add an extra ?
            // to avoid returning Option<Option<_>>
            let try_op = (self.is_nullable() && !self.is_array()).then(|| quote!(?));
            quote!( self.#base_method().map(|x| x #convert_impl) #try_op )
        } else {
            quote!( self.#base_method() #convert_impl )
        };

        Some(quote! {
            #[doc = #docs]
            pub fn #getter_name #decl_lifetime_if_needed (&self #input_data_if_needed) -> #return_type #where_read_clause {
                #data_alias_if_needed
                #args_if_needed
                #getter_impl
            }
        })
    }

    fn is_offset_or_array_of_offsets(&self) -> bool {
        match &self.typ {
            FieldType::Offset { .. } => true,
            FieldType::Array { inner_typ }
                if matches!(inner_typ.as_ref(), FieldType::Offset { .. }) =>
            {
                true
            }
            _ => false,
        }
    }

    pub(crate) fn offset_getter_name(&self) -> Option<syn::Ident> {
        if !self.is_offset_or_array_of_offsets() {
            return None;
        }
        if let Some(getter) = &self.attrs.offset_getter {
            return Some(getter.attr.clone());
        }

        let name_string = self.name.to_string();
        if name_string.ends_with('s') {
            // if this is an array of offsets (is pluralized) we also pluralize the getter name
            let temp = name_string.trim_end_matches("_offsets");
            // hacky attempt to respect pluralization rules. we can update this
            // as we encounter actual tables, instead of trying to be systematic
            let plural_es = temp.ends_with("attach");
            let suffix = if plural_es { "es" } else { "s" };
            Some(syn::Ident::new(
                &format!("{temp}{suffix}"),
                self.name.span(),
            ))
        } else {
            Some(syn::Ident::new(
                name_string.trim_end_matches("_offset"),
                self.name.span(),
            ))
        }
    }

    /// if the `#[offset_data_method]` attribute is specified, self.#method(),
    /// else return self.offset_data().
    ///
    /// This does not make sense in records.
    fn offset_getter_data_src(&self) -> TokenStream {
        match self.attrs.offset_data.as_ref() {
            Some(Attr { attr, .. }) => quote!(self.#attr()),
            None => quote!(self.offset_data()),
        }
    }

    /// the code generated for this field to validate data at parse time.
    pub(crate) fn field_parse_validation_stmts(&self) -> TokenStream {
        let name = &self.name;
        // handle the trivial case
        if !self.read_at_parse_time
            && !self.has_computed_len()
            && !self.validate_at_parse()
            && !self.is_version_dependent()
        {
            let typ = self.typ.cooked_type_tokens();
            return quote!( cursor.advance::<#typ>(); );
        }

        let versioned_field_start = self.attrs.available.as_ref().map(|available|{
            let field_start_name = self.shape_byte_start_field_name();
            quote! ( let #field_start_name = version.compatible(#available).then(|| cursor.position()).transpose()?; )
        });

        let other_stuff = if self.has_computed_len() {
            let len_expr = self.computed_len_expr().unwrap();
            let len_field_name = self.shape_byte_len_field_name();

            match &self.attrs.available {
                Some(version) => quote! {
                    let #len_field_name = version.compatible(#version).then_some(#len_expr);
                    if let Some(value) = #len_field_name {
                        cursor.advance_by(value);
                    }
                },
                None => quote! {
                    let #len_field_name = #len_expr;
                    cursor.advance_by(#len_field_name);
                },
            }
        } else if let Some(available) = &self.attrs.available {
            assert!(!self.is_array());
            let typ = self.typ.cooked_type_tokens();
            if self.read_at_parse_time {
                quote! {
                    let #name = version.compatible(#available).then(|| cursor.read::<#typ>()).transpose()?.unwrap_or(0);
                }
            } else {
                quote! {
                    version.compatible(#available).then(|| cursor.advance::<#typ>());
                }
            }
        } else if self.read_at_parse_time {
            let typ = self.typ.cooked_type_tokens();
            quote! ( let #name: #typ = cursor.read()?; )
        } else {
            panic!("who wrote this garbage anyway?");
        };

        quote! {
            #versioned_field_start
            #other_stuff
        }
    }

    /// The computed length of this field, if it is not a scalar/offset
    fn computed_len_expr(&self) -> Option<TokenStream> {
        if !self.has_computed_len() {
            return None;
        }

        assert!(!self.read_at_parse_time, "i did not expect this to happen");
        let read_args = self
            .attrs
            .read_with_args
            .as_deref()
            .map(FieldReadArgs::to_tokens_for_validation);

        if let FieldType::Struct { typ } = &self.typ {
            return Some(quote!( <#typ as ComputeSize>::compute_size(&#read_args)));
        }
        if let FieldType::PendingResolution { .. } = &self.typ {
            panic!("Should have resolved {:?}", self)
        }
        let len_expr = match self.attrs.count.as_deref() {
            Some(Count::All) => quote!(cursor.remaining_bytes()),
            Some(other) => {
                let count_expr = other.count_expr();
                let size_expr = match &self.typ {
                    FieldType::Array { inner_typ } => {
                        let inner_typ = inner_typ.cooked_type_tokens();
                        quote!( #inner_typ::RAW_BYTE_LEN )
                    }
                    FieldType::ComputedArray(array) => {
                        let inner = array.raw_inner_type();
                        quote!( <#inner as ComputeSize>::compute_size(&#read_args) )
                    }
                    _ => unreachable!("count not valid here"),
                };
                quote!(  #count_expr * #size_expr )
            }
            None => quote!(compile_error!("missing count attribute?")),
        };
        Some(len_expr)
    }

    pub(crate) fn record_len_expr(&self) -> TokenStream {
        self.computed_len_expr().unwrap_or_else(|| {
            let cooked = self.typ.cooked_type_tokens();
            quote!(#cooked::RAW_BYTE_LEN)
        })
    }

    pub(crate) fn record_init_stmt(&self) -> TokenStream {
        let name = &self.name;
        let rhs = match &self.typ {
            FieldType::Array { .. } => {
                let count_expr = self.attrs.count.as_ref().unwrap().count_expr();
                quote!(cursor.read_array(#count_expr)?)
            }
            FieldType::ComputedArray(_) => {
                let args = self
                    .attrs
                    .read_with_args
                    .as_ref()
                    .unwrap()
                    .to_tokens_for_validation();
                let count = self.attrs.count.as_ref().unwrap().count_expr();
                quote!(cursor.read_computed_array(#count, &#args)?)
            }
            _ => match self
                .attrs
                .read_with_args
                .as_deref()
                .map(FieldReadArgs::to_tokens_for_validation)
            {
                Some(args) => quote!(cursor.read_with_args(&#args)?),
                None => quote!(cursor.read()?),
            },
        };
        quote!( #name : #rhs )
    }

    /// 'None' if this field's value is computed at compile time
    fn compile_field_decl(&self) -> Option<TokenStream> {
        if self.is_computed() {
            return None;
        }

        let name = &self.name;
        let docs = &self.attrs.docs;
        let typ = self.owned_type();
        Some(quote!( #( #docs)* pub #name: #typ ))
    }

    fn supports_derive_default(&self) -> syn::Result<bool> {
        if self.attrs.default.is_some() {
            return Ok(false);
        }
        // this should maybe be in sanity check, but it's easier here, because
        // this is only called if codegen is running in 'compile' mode; if we
        // aren't in compile mode then this isn't an error.
        if let Some(version) = &self.attrs.version {
            if self.attrs.compile.is_none() {
                return Err(syn::Error::new(
                    version.span(),
                    "version field needs explicit #[default(x)] or #[compile(x)] attribute.",
                ));
            }
        }
        Ok(true)
    }

    /// 'None' if this field's value is computed at compile time
    fn compile_default_init(&self) -> Option<TokenStream> {
        if self.is_computed() {
            return None;
        }

        let name = &self.name;
        if let Some(expr) = self.attrs.default.as_deref() {
            Some(quote!( #name: #expr ))
        } else {
            Some(quote!( #name: Default::default() ))
        }
    }

    fn compile_write_stmt(&self) -> TokenStream {
        let mut computed = true;
        let value_expr = if let Some(format) = &self.attrs.format {
            let typ = self.typ.cooked_type_tokens();
            quote!(#format as #typ)
        } else if let Some(computed) = &self.attrs.compile {
            match &computed.attr {
                CustomCompile::Expr(inline_expr) => {
                    let typ = self.typ.cooked_type_tokens();
                    let expr = inline_expr.compile_expr();
                    if !inline_expr.referenced_fields.is_empty() {
                        quote!(#expr.unwrap() as #typ)
                    } else {
                        quote!(#expr as #typ)
                    }
                }
                // noop
                CustomCompile::Skip => return Default::default(),
            }
        } else {
            computed = false;
            let name = &self.name;
            quote!( self.#name )
        };

        let write_expr = if self.attrs.version.is_some() {
            quote! {
                let version = #value_expr;
                version.write_into(writer)
            }
        } else {
            let value_expr = if computed {
                quote!((#value_expr))
            } else {
                value_expr
            };

            if let Some(avail) = self.attrs.available.as_ref() {
                let needs_unwrap =
                    !(self.is_computed() || (self.attrs.nullable.is_some() && !self.is_array()));
                let expect = needs_unwrap.then(
                    || quote!(.as_ref().expect("missing versioned field should have failed validation")),
                );
                quote!(version.compatible(#avail).then(|| #value_expr #expect .write_into(writer)))
            } else {
                quote!(#value_expr.write_into(writer))
            }
        };

        if let Some(expr) = self.attrs.offset_adjustment.as_ref() {
            let expr = expr.compile_expr();
            quote! {
                writer.adjust_offsets(#expr, |writer| { #write_expr; })
            }
        } else {
            write_expr
        }
    }

    fn compile_write_contains_int_cast(&self) -> bool {
        self.attrs.format.is_some() || self.attrs.compile.is_some()
    }

    fn gets_recursive_validation(&self) -> bool {
        match &self.typ {
            FieldType::Scalar { .. } | FieldType::Struct { .. } => false,
            FieldType::Offset { target: None, .. } => false,
            FieldType::Offset {
                target: Some(OffsetTarget::Array(elem)),
                ..
            } if matches!(elem.deref(), FieldType::Scalar { .. }) => false,
            FieldType::Offset { .. }
            | FieldType::ComputedArray { .. }
            | FieldType::VarLenArray(_) => true,
            FieldType::Array { inner_typ } => matches!(
                inner_typ.as_ref(),
                FieldType::Offset { .. } | FieldType::Struct { .. }
            ),
            FieldType::PendingResolution { typ } => {
                panic!("Should have resolved {}", quote! { #typ });
            }
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_obj_requires_offset_data(&self, in_record: bool) -> bool {
        match &self.typ {
            _ if self.attrs.to_owned.is_some() => false,
            FieldType::Offset {
                target: Some(OffsetTarget::Array(_)),
                ..
            } => true,
            FieldType::Offset { .. } => in_record,
            FieldType::ComputedArray(_) | FieldType::VarLenArray(_) => true,
            FieldType::Struct { .. } => true,
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { .. } => in_record,
                FieldType::Struct { .. } | FieldType::Scalar { .. } => true,
                _ => false,
            },
            _ => false,
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_obj_ref_stmt(&self, in_record: bool) -> Option<TokenStream> {
        if self.is_computed() {
            return None;
        }

        let pass_offset_data = in_record.then(|| quote!(offset_data));
        let name = &self.name;
        let init_stmt = match &self.typ {
            _ if self.attrs.to_owned.is_some() => {
                self.attrs.to_owned.as_ref().unwrap().expr.to_token_stream()
            }
            FieldType::Scalar { .. } => quote!(obj.#name()),
            FieldType::Struct { .. } => quote!(obj.#name().to_owned_obj(offset_data)),
            FieldType::Offset {
                target: Some(target),
                ..
            } => {
                let offset_getter = self.offset_getter_name().unwrap();
                match target {
                    // in this case it is possible that this is an array of
                    // records that could contain offsets
                    OffsetTarget::Array(_) => {
                        quote!(obj.#offset_getter(#pass_offset_data).to_owned_obj(offset_data))
                    }
                    OffsetTarget::Table(_) => {
                        quote!(obj.#offset_getter(#pass_offset_data).to_owned_table())
                    }
                }
            }
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Scalar { .. } | FieldType::Struct { .. } => {
                    quote!(obj.#name().to_owned_obj(offset_data))
                }
                FieldType::Offset { .. } => {
                    let offset_getter = self.offset_getter_name().unwrap();
                    let getter = quote!(obj.#offset_getter(#pass_offset_data));
                    let converter = quote!(.map(|x| x.to_owned_table()).collect());
                    if self.attrs.available.is_some() {
                        quote!(#getter.map(|obj| obj #converter))
                    } else {
                        quote!(#getter #converter)
                    }
                }
                _ => quote!(compile_error!(
                    "unknown array type requires custom to_owned impl"
                )),
            },
            FieldType::ComputedArray(_) | FieldType::VarLenArray(_) => {
                let getter = quote!(obj.#name());
                let converter = quote!( .iter().filter_map(|x| x.map(|x| FromObjRef::from_obj_ref(&x, offset_data)).ok()).collect() );
                if self.attrs.available.is_some() {
                    quote!(#getter.map(|obj| obj #converter))
                } else {
                    quote!(#getter #converter)
                }
            }
            _ => quote!(compile_error!("requires custom to_owned impl")),
        };
        Some(quote!( #name: #init_stmt ))
    }
}

impl FieldType {
    /// 'cooked', as in now 'raw', i.e no 'BigEndian' wrapper
    pub(crate) fn cooked_type_tokens(&self) -> &syn::Ident {
        match &self {
            FieldType::Offset { typ, .. }
            | FieldType::Scalar { typ }
            | FieldType::Struct { typ } => typ,
            FieldType::PendingResolution { .. } => {
                panic!("Should never cook a type pending resolution {:#?}", self);
            }
            FieldType::Array { .. }
            | FieldType::ComputedArray { .. }
            | FieldType::VarLenArray(_) => {
                panic!("array tokens never cooked")
            }
        }
    }

    fn compile_type(&self, nullable: bool, version_dependent: bool) -> TokenStream {
        let raw_type = match self {
            FieldType::Scalar { typ } => typ.into_token_stream(),
            FieldType::Struct { typ } => typ.into_token_stream(),
            FieldType::Offset { typ, target } => {
                let target = target
                    .as_ref()
                    .map(OffsetTarget::compile_type)
                    .unwrap_or_else(|| quote!(Box<dyn FontWrite>));
                let width = width_for_offset(typ);
                if nullable {
                    // we don't bother wrapping this in an Option if versioned,
                    // since it already acts like an Option
                    return quote!(NullableOffsetMarker<#target, #width>);
                } else {
                    quote!(OffsetMarker<#target, #width>)
                }
            }
            FieldType::Array { inner_typ } => {
                if matches!(inner_typ.as_ref(), &FieldType::Array { .. }) {
                    panic!("nesting arrays is not supported");
                }

                let inner_tokens = inner_typ.compile_type(nullable, false);
                quote!( Vec<#inner_tokens> )
            }
            FieldType::ComputedArray(array) | FieldType::VarLenArray(array) => array.compile_type(),
            FieldType::PendingResolution { .. } => panic!("Should have resolved {:?}", self),
        };
        if version_dependent {
            quote!( Option<#raw_type> )
        } else {
            raw_type
        }
    }
}

fn width_for_offset(offset: &syn::Ident) -> Option<syn::Ident> {
    if offset == "Offset16" {
        // the default
        None
    } else if offset == "Offset24" {
        Some(syn::Ident::new("WIDTH_24", offset.span()))
    } else if offset == "Offset32" {
        Some(syn::Ident::new("WIDTH_32", offset.span()))
    } else {
        panic!("not an offset: {offset}")
    }
}

impl FieldReadArgs {
    fn to_tokens_for_table_getter(&self) -> TokenStream {
        match self.inputs.as_slice() {
            [arg] => quote!(self.#arg()),
            args => quote!( ( #( self.#args() ),* ) ),
        }
    }

    fn to_tokens_for_validation(&self) -> TokenStream {
        match self.inputs.as_slice() {
            [arg] => arg.to_token_stream(),
            args => quote!( ( #( #args ),* ) ),
        }
    }
}
