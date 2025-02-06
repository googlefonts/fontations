//! methods on fields

use std::{borrow::Cow, ops::Deref};

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::spanned::Spanned;

use super::parsing::{
    logged_syn_error, Attr, Condition, Count, CountArg, CustomCompile, Field, FieldReadArgs,
    FieldType, FieldValidation, Fields, IfTransform, NeededWhen, OffsetTarget, Phase, Record,
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
                        return Err(logged_syn_error(fld.name.span(), format!("field has custom offset data, but previous field '{}' already specified different custom offset data", prev_field.name)));
                    }
                }
            } else if fld.is_offset_or_array_of_offsets() {
                normal_offset_data_fld = Some(fld);
            }

            // Ideally we'd map is_offset => _offset and array of offsets => _offsets but STAT::offsetToAxisValueOffsets breaks the rule
            if fld.is_offset_or_array_of_offsets() {
                let name = fld.name.to_string();
                if !name.ends_with("_offset") && !name.ends_with("_offsets") {
                    return Err(logged_syn_error(
                        fld.name.span(),
                        "name must end in _offset or _offsets",
                    ));
                }
            }

            // We can't generate a compile type if don't define a way to have value
            if !fld.has_defined_value() {
                return Err(logged_syn_error(
                    fld.name.span(),
                    "There is no defined way to get a value. If you are skipping getter then perhaps you have a fixed value, such as for a reserved field that should be set to 0? - if so please use #[compile(0)]",
                ));
            }

            if matches!(fld.attrs.count.as_deref(), Some(Count::All(_)))
                && i != self.fields.len() - 1
            {
                return Err(logged_syn_error(
                    fld.name.span(),
                    "#[count(..)] or VarLenArray fields can only be last field in table.",
                ));
            }
            fld.sanity_check(phase)?;
        }

        if let (Some(custom), Some(normal)) = (custom_offset_data_fld, normal_offset_data_fld) {
            return Err(logged_syn_error(custom.name.span(), format!("Not implemented: field requires custom offset data, but sibling field '{}' expects default offset data", normal.name)));
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

    /// return the names of any fields that are the inputs of a conditional statement
    /// on another field.
    pub(crate) fn conditional_input_idents(&self) -> Vec<syn::Ident> {
        let mut result = self
            .fields
            .iter()
            .flat_map(|fld| fld.attrs.conditional.as_ref())
            .flat_map(|cond| cond.input_field())
            .collect::<Vec<_>>();
        result.sort();
        result.dedup();
        result
    }

    pub(crate) fn iter_constructor_info(&self) -> impl Iterator<Item = FieldConstructorInfo> + '_ {
        self.fields.iter().filter_map(|fld| {
            fld.compile_constructor_arg_type()
                .map(|arg| FieldConstructorInfo {
                    name: fld.name_for_compile().into_owned(),
                    arg_tokens: arg,
                    is_offset: fld.is_offset_or_array_of_offsets(),
                    is_array: fld.is_array(),
                    manual_compile_type: fld.attrs.compile_type.is_some(),
                })
        })
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
        self.iter().find(|fld| fld.attrs.version.is_some())
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
            let name = field.name_for_compile();
            let name_str = name.to_string();
            let validation_call = match field.attrs.validate.as_deref() {
                Some(FieldValidation::Skip) => continue,
                Some(FieldValidation::Custom(ident)) => Some(quote!( self.#ident(ctx); )),
                None if field.gets_recursive_validation() => {
                    Some(quote!( self.#name.validate_impl(ctx); ))
                }
                None => None,
            };

            let is_single_nullable_offset = field.is_nullable() && !field.is_array();
            let is_conditional = field
                .attrs
                .conditional
                .as_ref()
                .filter(|_| !is_single_nullable_offset)
                .map(|attr| {
                    let condition = attr.condition_tokens_for_read();
                    match &attr.attr {
                        Condition::SinceVersion(_) => quote! {
                            if #condition && self.#name.is_none() {
                                ctx.report(format!("field must be present for version {version}"));
                            }
                        },
                        Condition::IfFlag { flag, .. } => {
                            let flag = stringify_path(flag);
                            let flag_missing = format!("'{name}' is present but {flag} not set",);
                            let field_missing = format!("{flag} is set but '{name}' is None",);
                            quote! {
                                if !(#condition) && self.#name.is_some() {
                                    ctx.report(#flag_missing)
                                }
                                if (#condition) && self.#name.is_none() {
                                    ctx.report(#field_missing)
                                }
                            }
                        }
                        Condition::IfCond { xform } => match xform {
                            IfTransform::AnyFlag(_, _) => {
                                let condition_not_set_message =
                                    format!("if_cond is not satisfied but '{name}' is present.");
                                let condition_set_message =
                                    format!("if_cond is satisfied by '{name}' is not present.");
                                quote! {
                                    if !(#condition) && self.#name.is_some() {
                                        ctx.report(#condition_not_set_message);
                                    }
                                    if (#condition) && self.#name.is_none() {
                                        ctx.report(#condition_set_message);
                                    }
                                }
                            }
                        },
                    }
                });

            // this all deals with the case where we have an optional field.
            let maybe_check_is_some = is_conditional
                .is_some()
                .then(|| quote!(self.#name.is_some() &&));
            let maybe_unwrap = is_conditional.is_some().then(|| quote!(.as_ref().unwrap()));

            let array_len_check = if let Some(ident) =
                field.attrs.count.as_deref().and_then(Count::single_field)
            {
                let typ = self.get_scalar_field_type(ident);
                Some(quote! {
                    if #maybe_check_is_some self.#name #maybe_unwrap.len() > (#typ::MAX as usize) {
                        ctx.report("array exceeds max length");
                    }
                })
            } else {
                None
            };

            if validation_call.is_some() || array_len_check.is_some() || is_conditional.is_some() {
                stmts.push(quote! {
                    ctx.in_field(#name_str, |ctx| {
                        #is_conditional
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
                    .conditional
                    .as_ref()
                    .map(|v| v.condition_tokens_for_read())
                    .map(|cond| quote!( if #cond ));
                let rhs = traversal_arm_for_field(fld, in_record, pass_data.as_ref());
                quote!( #i #condition => Some(#rhs) )
            })
    }
}

fn if_expression(xform: &IfTransform, add_self: bool) -> TokenStream {
    match xform {
        IfTransform::AnyFlag(field, flags) => {
            if add_self {
                quote!(self.#field.intersects(#(#flags)|*))
            } else {
                quote!(#field.intersects(#(#flags)|*))
            }
        }
    }
}

impl Condition {
    fn condition_tokens_for_read(&self) -> TokenStream {
        match self {
            Condition::SinceVersion(version) => quote!(version.compatible(#version)),
            Condition::IfFlag { field, flag } => quote!(#field.contains(#flag)),
            Condition::IfCond { xform } => if_expression(xform, false),
        }
    }

    fn condition_tokens_for_write(&self) -> TokenStream {
        match self {
            Condition::SinceVersion(version) => quote!(version.compatible(#version)),
            Condition::IfFlag { field, flag } => quote!(self.#field.contains(#flag)),
            Condition::IfCond { xform } => if_expression(xform, true),
        }
    }

    /// the name of any fields that need to be instantiated to determine this condition
    fn input_field(&self) -> Vec<syn::Ident> {
        match self {
            // special case, we always treat a version field as input
            Condition::SinceVersion(_) => vec![],
            Condition::IfFlag { field, .. } => vec![field.clone()],
            Condition::IfCond { xform } => xform.input_field(),
        }
    }
}

/// All the state required to generate a constructor for a table/record
/// that includes this field.
pub(crate) struct FieldConstructorInfo {
    pub(crate) name: syn::Ident,
    pub(crate) arg_tokens: TokenStream,
    pub(crate) is_offset: bool,
    pub(crate) is_array: bool,
    pub(crate) manual_compile_type: bool,
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
    let maybe_unwrap = fld.attrs.conditional.is_some().then(|| quote!(.unwrap()));
    let maybe_unwrap_getter = maybe_unwrap.as_ref().filter(|_| !fld.is_nullable());
    if let Some(traverse_with) = &fld.attrs.traverse_with {
        let traverse_fn = &traverse_with.attr;
        if traverse_fn == "skip" {
            return quote!(Field::new(#name_str, traversal::FieldType::Unknown));
        }
        return quote!(Field::new(#name_str, self.#traverse_fn(#pass_data)));
    }
    match &fld.typ {
        FieldType::Offset {
            target: OffsetTarget::Array(inner),
            ..
        } if matches!(inner.deref(), FieldType::Struct { .. }) => {
            let typ = inner.cooked_type_tokens();
            let getter = fld.offset_getter_name();
            let offset_data = pass_data
                .cloned()
                .unwrap_or_else(|| fld.offset_getter_data_src());
            quote!(Field::new(
                    #name_str,
                    traversal::FieldType::offset_to_array_of_records(
                        self.#name()#maybe_unwrap,
                        self.#getter(#pass_data)#maybe_unwrap_getter,
                        stringify!(#typ),
                        #offset_data,
                    )
            ))
        }
        FieldType::Offset { target, .. } => {
            let constructor_name = match target {
                OffsetTarget::Table(_) => quote!(offset),
                OffsetTarget::Array(_) => quote!(offset_to_array_of_scalars),
            };
            let getter = fld.offset_getter_name();
            quote!(Field::new(#name_str, FieldType::#constructor_name(self.#name()#maybe_unwrap, self.#getter(#pass_data)#maybe_unwrap_getter)))
        }
        FieldType::Scalar { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),

        FieldType::Array { inner_typ } => match inner_typ.as_ref() {
            FieldType::Scalar { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),
            //HACK: glyf has fields that are [u8]
            FieldType::Struct { typ } if typ == "u8" => {
                quote!(Field::new( #name_str, self.#name()#maybe_unwrap))
            }
            FieldType::Struct { typ } => {
                let offset_data = pass_data
                    .cloned()
                    .unwrap_or_else(|| fld.offset_getter_data_src());
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
                target: OffsetTarget::Table(target),
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
                target: OffsetTarget::Array(_),
                ..
            } => panic!("achievement unlocked: 'added arrays of offsets to arrays to OpenType spec' {fld:#?}"),
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
        FieldType::VarLenArray(arr) => {
            let typ_str = arr.raw_inner_type().to_string();
            let data = fld.offset_getter_data_src();
            quote!(Field::new(#name_str, traversal::FieldType::var_array(#typ_str, self.#name()#maybe_unwrap, #data)))
        }
        // See if there are better ways to handle these hardcoded types
        // <https://github.com/googlefonts/fontations/issues/659>
        FieldType::Struct { typ } if typ == "ValueRecord" || typ == "SbitLineMetrics" => {
            let offset_data = pass_data
                .cloned()
                .unwrap_or_else(|| fld.offset_getter_data_src());
            quote!(Field::new(#name_str, self.#name() #maybe_unwrap .traversal_type(#offset_data)))
        }
        FieldType::Struct { .. } => {
            quote!(compile_error!(concat!("another weird type: ", #name_str)))
        }
        FieldType::PendingResolution { .. } => panic!("Should have resolved {fld:#?}"),
    }
}

fn check_resolution(phase: Phase, field_type: &FieldType) -> syn::Result<()> {
    if let Phase::Parse = phase {
        return Ok(());
    }
    if let FieldType::PendingResolution { typ } = field_type {
        return Err(logged_syn_error(
            typ.span(),
            format!("{typ}: be ye struct or scalar? - we certainly don't know.",),
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
                FieldType::Struct { typ } => quote!( &'a [#typ] ),
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

    pub(crate) fn is_zerocopy_compatible(&self) -> bool {
        // hack: we want to add `FieldType::Struct` here but don't want to
        // catch `ValueRecord` so use this attribute to ignore it.
        // Fields that require args for reading can't be read "zerocopy"
        // anyway.
        // <https://github.com/googlefonts/fontations/issues/659>
        self.attrs.read_with_args.is_none()
            && matches!(
                self.typ,
                FieldType::Scalar { .. } | FieldType::Offset { .. } | FieldType::Struct { .. }
            )
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

    pub(crate) fn is_conditional(&self) -> bool {
        self.attrs.conditional.is_some()
    }

    /// Sanity check we are in a sane state for the end of phase
    fn sanity_check(&self, phase: Phase) -> syn::Result<()> {
        check_resolution(phase, &self.typ)?;
        if let FieldType::Array { inner_typ } = &self.typ {
            if matches!(
                inner_typ.as_ref(),
                FieldType::Array { .. } | FieldType::ComputedArray(_)
            ) {
                return Err(logged_syn_error(
                    self.name.span(),
                    "nested arrays are not allowed",
                ));
            }
            check_resolution(phase, inner_typ)?;
        }
        if let FieldType::Offset {
            target: OffsetTarget::Array(inner_typ),
            ..
        } = &self.typ
        {
            check_resolution(phase, inner_typ)?;
        }
        if self.is_array() && self.attrs.count.is_none() {
            return Err(logged_syn_error(
                self.name.span(),
                "array requires #[count] attribute",
            ));
        }
        if let Some(args) = &self.attrs.read_with_args {
            match &self.typ {
                FieldType::ComputedArray(array) if self.attrs.count.is_none() => {
                    return Err(logged_syn_error(array.span(), "missing count attribute"));
                }
                FieldType::Offset { .. } => (),
                FieldType::Array { inner_typ, .. }
                    if matches!(inner_typ.as_ref(), FieldType::Offset { .. }) => {}
                FieldType::Scalar { .. } | FieldType::Array { .. } => {
                    return Err(logged_syn_error(
                        args.span(),
                        "attribute not valid on this type",
                    ))
                }
                _ => (),
            }
        }

        if let Some(comp_attr) = &self.attrs.compile_with {
            if self.attrs.compile.is_some() {
                return Err(logged_syn_error(
                    comp_attr.span(),
                    "cannot have both 'compile' and 'compile_with'",
                ));
            }
        }

        if self.attrs.version.is_some() && self.name != "version" {
            // this seems to be always true and it simplifies our lives; if not
            // we will need to handle custom idents in more places.
            return Err(logged_syn_error(
                self.attrs.version.as_ref().unwrap().span(),
                "#[version] attribute expects to be on field named 'version",
            ));
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
                let try_op = self.is_conditional().then(|| quote!(?));
                quote!(self.#len_field #try_op)
            }
            FieldType::PendingResolution { .. } => panic!("Should have resolved {self:?}"),
        }
    }

    /// iterate the names of fields that are required for parsing or instantiating
    /// this field.
    fn input_fields(&self) -> Vec<(syn::Ident, NeededWhen)> {
        let mut result = Vec::new();
        if let Some(count) = self.attrs.count.as_ref() {
            result.extend(
                count
                    .iter_referenced_fields()
                    .map(|fld| (fld.clone(), NeededWhen::Parse)),
            );
        }
        if let Some(flds) = self.attrs.conditional.as_ref().map(|c| c.input_field()) {
            for fld in flds {
                result.push((fld, NeededWhen::Parse))
            }
        }

        if let Some(read_with) = self.attrs.read_with_args.as_ref() {
            result.extend(
                read_with
                    .inputs
                    .iter()
                    .map(|fld| (fld.clone(), NeededWhen::Both)),
            );
        }
        if let Some(read_offset) = self.attrs.read_offset_args.as_ref() {
            result.extend(
                read_offset
                    .inputs
                    .iter()
                    .map(|fld| (fld.clone(), NeededWhen::Runtime)),
            );
        }
        result
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
            FieldType::PendingResolution { .. } => panic!("Should have resolved {self:?}"),
        }
    }

    /// the type as it appears in a `write-fonts` struct.
    pub(crate) fn owned_type(&self) -> TokenStream {
        if let Some(typ) = &self.attrs.compile_type {
            return typ.into_token_stream();
        }
        let type_tokens = self.typ.compile_type_tokens(self.is_nullable());

        // if this is versioned, we wrap in `Option``
        if self.is_conditional() {
            // UNLESS this is a NullableOffsetMarker, where is already basically an Option
            if !(self.is_nullable() && matches!(self.typ, FieldType::Offset { .. })) {
                return quote!(Option<#type_tokens>);
            }
        }
        type_tokens
    }

    pub(crate) fn table_getter_return_type(&self) -> Option<TokenStream> {
        if !self.has_getter() {
            return None;
        }
        let return_type = self.raw_getter_return_type();
        if self.is_conditional() {
            Some(quote!(Option<#return_type>))
        } else {
            Some(return_type)
        }
    }
    pub(crate) fn table_getter(&self, generic: Option<&syn::Ident>) -> Option<TokenStream> {
        let return_type = self.table_getter_return_type()?;
        let name = &self.name;
        let is_array = self.is_array();
        let is_var_array = self.is_var_array();
        let is_versioned = self.is_conditional();

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
                panic!("Should have resolved {self:?}")
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
        let try_op = self.is_conditional().then(|| quote!(?));
        quote!( self.shape.#shape_range_fn_name() #try_op )
    }

    fn typed_offset_getter_docs(&self, has_data_arg: bool) -> TokenStream {
        let raw_name = &self.name;
        // If there's no arguments than we just link to the raw offset method
        if !has_data_arg {
            let docs = if self.is_array() {
                format!(" A dynamically resolving wrapper for [`{raw_name}`][Self::{raw_name}].")
            } else {
                format!(" Attempt to resolve [`{raw_name}`][Self::{raw_name}].")
            };
            return quote!(#[doc = #docs]);
        }

        // if there is a data argument than we want to be more explicit
        let original_docs = &self.attrs.docs;

        quote! {
            #(#original_docs)*
            #[doc = ""]
            #[doc = " The `data` argument should be retrieved from the parent table"]
            #[doc = " By calling its `offset_data` method."]
        }
    }

    fn typed_offset_field_getter(
        &self,
        generic: Option<&syn::Ident>,
        record: Option<&Record>,
    ) -> Option<TokenStream> {
        let (offset_type, target) = match &self.typ {
            _ if self.attrs.offset_getter.is_some() => return None,
            FieldType::Offset { typ, target } => (typ, target),
            FieldType::Array { inner_typ, .. } => match inner_typ.as_ref() {
                FieldType::Offset { typ, target } => (typ, target),
                _ => return None,
            },
            _ => return None,
        };

        let raw_name = &self.name;
        let getter_name = self.offset_getter_name().unwrap();
        let target_is_generic =
            matches!(target, OffsetTarget::Table(ident) if Some(ident) == generic);
        let where_read_clause = target_is_generic.then(|| quote!(where T: FontRead<'a>));
        // if a record, data is passed in
        let input_data_if_needed = record.is_some().then(|| quote!(, data: FontData<'a>));
        let decl_lifetime_if_needed =
            record.and_then(|x| x.lifetime.is_none().then(|| quote!(<'a>)));

        // if a table, data is self.data, else it is passed as an argument
        let data_alias_if_needed = record.is_none().then(|| quote!(let data = self.data;));

        let args_if_needed = self.attrs.read_offset_args.as_ref().map(|args| {
            let args = args.to_tokens_for_table_getter();
            quote!(let args = #args;)
        });
        let docs = self.typed_offset_getter_docs(record.is_some());

        if self.is_array() {
            let OffsetTarget::Table(target_ident) = target else {
                panic!("I don't think arrays of offsets to arrays are in the spec?");
            };
            let array_type = if self.is_nullable() {
                quote!(ArrayOfNullableOffsets)
            } else {
                quote!(ArrayOfOffsets)
            };

            let target_lifetime = (!target_is_generic).then(|| quote!(<'a>));

            let args_token = if self.attrs.read_offset_args.is_some() {
                quote!(args)
            } else {
                quote!(())
            };
            let mut return_type =
                quote!( #array_type<'a, #target_ident #target_lifetime, #offset_type> );
            let mut body = quote!(#array_type::new(offsets, data, #args_token));
            if self.is_conditional() {
                return_type = quote!( Option< #return_type > );
                body = quote!( offsets.map(|offsets| #body ) );
            }

            let bind_offsets = quote!( let offsets = self.#raw_name(); );

            Some(quote! {
                #docs
                pub fn #getter_name (&self #input_data_if_needed) -> #return_type #where_read_clause  {
                    #data_alias_if_needed
                    #bind_offsets
                    #args_if_needed
                    #body
                }
            })
        } else {
            let mut return_type = target.getter_return_type(target_is_generic);
            if self.is_nullable() || self.attrs.conditional.is_some() {
                return_type = quote!(Option<#return_type>);
            }
            let resolve = match self.attrs.read_offset_args.as_deref() {
                None => quote!(resolve(data)),
                Some(_) => quote!(resolve_with_args(data, &args)),
            };
            let getter_impl = if self.is_conditional() {
                // if this is nullable *and* version dependent we add a `?`
                // to avoid returning Option<Option<_>>
                let try_op = self.is_nullable().then(|| quote!(?));
                quote!( self. #raw_name ().map(|x| x. #resolve) #try_op )
            } else {
                quote!( self. #raw_name () .#resolve )
            };
            Some(quote! {
                #docs
                pub fn #getter_name #decl_lifetime_if_needed (&self #input_data_if_needed) -> #return_type #where_read_clause {
                    #data_alias_if_needed
                    #args_if_needed
                    #getter_impl
                }
            })
        }
    }

    fn is_count(&self) -> bool {
        self.attrs.count.is_some()
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

    fn has_defined_value(&self) -> bool {
        if self.attrs.skip_getter.is_none() {
            return true;
        }
        self.is_computed() || self.is_count()
    }

    pub(crate) fn offset_getter_name(&self) -> Option<syn::Ident> {
        if !self.is_offset_or_array_of_offsets() {
            return None;
        }
        if let Some(getter) = &self.attrs.offset_getter {
            return Some(getter.attr.clone());
        }

        let name_string = self.name.to_string();
        let name_string = remove_offset_from_field_name(&name_string);
        Some(syn::Ident::new(&name_string, self.name.span()))
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
            && !self.is_conditional()
        {
            let typ = self.typ.cooked_type_tokens();
            return quote!( cursor.advance::<#typ>(); );
        }

        let conditional_field_start = self.attrs.conditional.as_ref().map(|condition| {
            let field_start_name = self.shape_byte_start_field_name();
            let condition = condition.condition_tokens_for_read();
            quote! ( let #field_start_name = #condition.then(|| cursor.position()).transpose()?; )
        });

        let other_stuff = if self.has_computed_len() {
            let len_expr = self.computed_len_expr().unwrap();
            let len_field_name = self.shape_byte_len_field_name();

            match &self.attrs.conditional {
                Some(condition) => {
                    let condition = condition.condition_tokens_for_read();
                    quote! {
                        let #len_field_name = #condition.then_some(#len_expr);
                        if let Some(value) = #len_field_name {
                            cursor.advance_by(value);
                        }
                    }
                }
                None => quote! {
                    let #len_field_name = #len_expr;
                    cursor.advance_by(#len_field_name);
                },
            }
        } else if let Some(condition) = &self.attrs.conditional {
            assert!(!self.is_array());
            let typ = self.typ.cooked_type_tokens();
            let condition = condition.condition_tokens_for_read();
            if self.read_at_parse_time {
                quote! {
                    let #name = #condition.then(|| cursor.read::<#typ>()).transpose()?.unwrap_or_default();
                }
            } else {
                quote! {
                    #condition.then(|| cursor.advance::<#typ>());
                }
            }
        } else if self.read_at_parse_time {
            let typ = self.typ.cooked_type_tokens();
            quote! ( let #name: #typ = cursor.read()?; )
        } else {
            panic!("who wrote this garbage anyway?");
        };

        quote! {
            #conditional_field_start
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
            return Some(quote!( <#typ as ComputeSize>::compute_size(&#read_args)? ));
        }
        if let FieldType::PendingResolution { .. } = &self.typ {
            panic!("Should have resolved {self:?}")
        }
        let len_expr = match self.attrs.count.as_deref() {
            Some(Count::All(_)) => {
                match &self.typ {
                    FieldType::Array { inner_typ } => {
                        let inner_typ = inner_typ.cooked_type_tokens();
                        // Make sure the remaining byte size is a multiple of
                        // the requested element type size.
                        // See <https://github.com/googlefonts/fontations/issues/797>
                        quote!(cursor.remaining_bytes() / #inner_typ::RAW_BYTE_LEN * #inner_typ::RAW_BYTE_LEN)
                    }
                    _ => quote!(cursor.remaining_bytes()),
                }
            }
            Some(other) => {
                let count_expr = other.count_expr();
                let size_expr = match &self.typ {
                    FieldType::Array { inner_typ } => {
                        let inner_typ = inner_typ.cooked_type_tokens();
                        quote!( #inner_typ::RAW_BYTE_LEN )
                    }
                    FieldType::ComputedArray(array) => {
                        let inner = array.raw_inner_type();
                        quote!( <#inner as ComputeSize>::compute_size(&#read_args)? )
                    }
                    FieldType::VarLenArray(array) => {
                        let inner = array.raw_inner_type();
                        return Some(quote! {
                            {
                                let data = cursor.remaining().ok_or(ReadError::OutOfBounds)?;
                                <#inner as VarSize>::total_len_for_count(data, #count_expr)?
                            }
                        });
                    }
                    _ => unreachable!("count not valid here"),
                };
                match other {
                    Count::SingleArg(CountArg::Literal(lit)) if lit.base10_digits() == "1" => {
                        // Prevent identity-op clippy error with `1 * size`
                        size_expr
                    }
                    _ => {
                        quote!(  (#count_expr).checked_mul(#size_expr).ok_or(ReadError::OutOfBounds)? )
                    }
                }
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
            FieldType::Scalar { typ } => {
                if typ == "u8" {
                    // We don't wrap u8 in BigEndian so we need to read it
                    // directly
                    quote!(cursor.read()?)
                } else {
                    quote!(cursor.read_be()?)
                }
            }
            _ => match self
                .attrs
                .read_with_args
                .as_deref()
                .map(FieldReadArgs::to_tokens_for_validation)
            {
                Some(args) => quote!(cursor.read_with_args(&#args)?),
                None => quote!(cursor.read_be()?),
            },
        };
        quote!( #name : #rhs )
    }

    fn name_for_compile(&self) -> Cow<syn::Ident> {
        self.offset_getter_name()
            .map(Cow::Owned)
            .unwrap_or(Cow::Borrowed(&self.name))
    }

    /// 'None' if this field's value is computed at compile time
    fn compile_field_decl(&self) -> Option<TokenStream> {
        if self.is_computed() {
            return None;
        }

        let name = self.name_for_compile();
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
                return Err(logged_syn_error(
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

        let name = self.name_for_compile();
        if let Some(expr) = self.attrs.default.as_deref() {
            Some(quote!( #name: #expr ))
        } else {
            Some(quote!( #name: Default::default() ))
        }
    }

    pub(crate) fn skipped_in_constructor(&self) -> bool {
        self.attrs.default.is_some() || self.is_conditional()
    }
    /// If this field should be part of a generated constructor, returns the type to use.
    ///
    /// We do not include arguments for types that are computed, or types that
    /// are only available in a specific version.
    fn compile_constructor_arg_type(&self) -> Option<TokenStream> {
        if self.skipped_in_constructor() || self.is_computed() {
            return None;
        }
        if let Some(typ) = self.attrs.compile_type.as_ref() {
            return Some(typ.into_token_stream());
        }

        Some(self.typ.compile_type_for_constructor(self.is_nullable()))
    }

    fn compile_write_stmt(&self) -> TokenStream {
        let mut computed = true;
        let value_expr = if let Some(format) = &self.attrs.format {
            let typ = self.typ.cooked_type_tokens();
            quote!(#format as #typ)
        } else if let Some(computed) = &self.attrs.compile {
            match &computed.attr {
                CustomCompile::Expr(inline_expr) => {
                    // this expects that the type is always some simple scalar,
                    // and does not work if there is an explicit #[compile_type]
                    // specified; it may need to be reevaluated at some point.
                    let typ = self.typ.cooked_type_tokens();
                    let expr = inline_expr.compile_expr();
                    if !inline_expr.referenced_fields.is_empty() {
                        quote!( #typ :: try_from(#expr).unwrap() )
                    } else {
                        quote!(#expr as #typ)
                    }
                }
                // noop
                CustomCompile::Skip => return Default::default(),
            }
        } else if let Some(attr) = self.attrs.compile_with.as_ref() {
            let method = &attr.attr;
            quote!( self.#method() )
        } else {
            computed = false;
            let name = self.name_for_compile();
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

            if let Some(condition) = self.attrs.conditional.as_ref() {
                let needs_unwrap =
                    !(self.is_computed() || (self.attrs.nullable.is_some() && !self.is_array()));
                let expect = needs_unwrap.then(
                    || quote!(.as_ref().expect("missing conditional field should have failed validation")),
                );
                let condition = condition.condition_tokens_for_write();
                quote!(#condition.then(|| #value_expr #expect .write_into(writer)))
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
            FieldType::Offset {
                target: OffsetTarget::Array(elem),
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
                target: OffsetTarget::Array(_),
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
        let name = self.name_for_compile();
        let init_stmt = match &self.typ {
            _ if self.attrs.to_owned.is_some() => {
                self.attrs.to_owned.as_ref().unwrap().expr.to_token_stream()
            }
            FieldType::Scalar { .. } => quote!(obj.#name()),
            FieldType::Struct { .. } => quote!(obj.#name().to_owned_obj(offset_data)),
            FieldType::Offset { target, .. } => {
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
                    let converter = quote!( .to_owned_table() );
                    if self.attrs.conditional.is_some() {
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
                if self.attrs.conditional.is_some() {
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
                panic!("Should never cook a type pending resolution {self:#?}");
            }
            FieldType::Array { .. }
            | FieldType::ComputedArray { .. }
            | FieldType::VarLenArray(_) => {
                panic!("array tokens never cooked")
            }
        }
    }

    /// The tokens used to represent this type in `write-fonts`
    ///
    /// This does not include the `Option` that may be present if this is versioned.
    fn compile_type_tokens(&self, nullable: bool) -> TokenStream {
        self.compile_type_impl(nullable, false)
    }

    /// The type for a constructor argument for this field.
    ///
    /// Specifically, this does not wrap offset-types in offset markers
    /// (these conversions occur in the constructor).
    fn compile_type_for_constructor(&self, nullable: bool) -> TokenStream {
        self.compile_type_impl(nullable, true)
    }

    // impl code reused for the two calls above
    fn compile_type_impl(&self, nullable: bool, for_constructor: bool) -> TokenStream {
        match self {
            FieldType::Scalar { typ } => typ.into_token_stream(),
            FieldType::Struct { typ } => typ.into_token_stream(),
            FieldType::Offset { typ, target } => {
                let target = target.compile_type();
                if for_constructor {
                    if nullable {
                        return quote!(Option<#target>);
                    } else {
                        return target;
                    }
                }
                let width = width_for_offset(typ);
                if nullable {
                    // we don't bother wrapping this in an Option if versioned,
                    // since it already acts like an Option
                    quote!(NullableOffsetMarker<#target, #width>)
                } else {
                    quote!(OffsetMarker<#target, #width>)
                }
            }
            FieldType::Array { inner_typ } => {
                if matches!(inner_typ.as_ref(), &FieldType::Array { .. }) {
                    panic!("nesting arrays is not supported");
                }

                let inner_tokens = inner_typ.compile_type_impl(nullable, for_constructor);
                quote!( Vec<#inner_tokens> )
            }
            FieldType::ComputedArray(array) | FieldType::VarLenArray(array) => array.compile_type(),
            FieldType::PendingResolution { .. } => panic!("Should have resolved {self:?}"),
        }
    }
}

// convert thing_offset -> thing, and thing_offsets -> things
pub(crate) fn remove_offset_from_field_name(name: &str) -> Cow<str> {
    if !(name.ends_with("_offset") || name.ends_with("_offsets")) {
        return Cow::Borrowed(name);
    }
    if name.ends_with('s') {
        // if this is an array of offsets (is pluralized) we also pluralize the getter name
        let temp = name.trim_end_matches("_offsets");
        // hacky attempt to respect pluralization rules. we can update this
        // as we encounter actual tables, instead of trying to be systematic
        let suffix = if temp.ends_with("attach") || temp.ends_with("patch") {
            "es"
        } else if temp.ends_with("data") {
            ""
        } else {
            "s"
        };
        Cow::Owned(format!("{temp}{suffix}"))
    } else {
        Cow::Borrowed(name.trim_end_matches("_offset"))
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

/// turn a syn path like 'std :: hmm :: Thing' into "Thing", for shorter diagnostic
/// messages.
fn stringify_path(path: &syn::Path) -> String {
    let s = path.to_token_stream().to_string();
    s.rsplit_once(' ')
        .map(|(_, end)| end.to_string())
        .unwrap_or(s)
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
