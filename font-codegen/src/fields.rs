//! methods on fields

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use crate::parsing::{Count, NeededWhen, ReferencedFields};

use super::parsing::{Field, FieldReadArgs, FieldType, Fields, Record};

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

    pub(crate) fn sanity_check(&self) -> syn::Result<()> {
        for fld in &self.fields {
            fld.sanity_check()?;
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

    // we use this to add disable a clippy lint if needed
    pub(crate) fn compile_write_contains_int_casts(&self) -> bool {
        self.fields
            .iter()
            .any(Field::compile_write_contains_int_cast)
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
            let name = &field.name;
            let name_str = field.name.to_string();
            let recursive_stmt = field
                .gets_recursive_validation()
                .then(|| quote!( self.#name.validate_impl(ctx); ));

            let array_len_check =
                if let Some(Count::Field(count_name)) = field.attrs.count.as_deref() {
                    let typ = self.get_scalar_field_type(count_name);
                    Some(quote! {
                        if self.#name.len() > (#typ::MAX as usize) {
                            ctx.report("array excedes max length");
                        }
                    })
                } else {
                    None
                };

            if recursive_stmt.is_some() || array_len_check.is_some() {
                stmts.push(quote! {
                    ctx.in_field(#name_str, |ctx| {
                        #array_len_check
                        #recursive_stmt
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

fn traversal_arm_for_field(
    fld: &Field,
    in_record: bool,
    pass_data: Option<&TokenStream>,
) -> TokenStream {
    let name_str = &fld.name.to_string();
    let name = &fld.name;
    let maybe_unwrap = fld.attrs.available.is_some().then(|| quote!(.unwrap()));
    match &fld.typ {
        FieldType::Offset {
            target: Some(_), ..
        } => {
            let getter = fld.offset_getter_name();
            quote!(Field::new(#name_str, FieldType::offset(self.#name()#maybe_unwrap, self.#getter(#pass_data)#maybe_unwrap)))
        }
        FieldType::Offset { .. } => {
            quote!(Field::new(#name_str, FieldType::unknown_offset(self.#name()#maybe_unwrap)))
        }
        FieldType::Scalar { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),

        FieldType::Array { inner_typ } => match inner_typ.as_ref() {
            FieldType::Scalar { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),
            //HACK: glyf has fields that are [u8]
            FieldType::Other { typ } if typ.is_ident("u8") => {
                quote!(Field::new( #name_str, self.#name()#maybe_unwrap))
            }
            FieldType::Other { typ } if !in_record => {
                quote!(Field::new(
                        #name_str,
                        traversal::FieldType::array_of_records(
                            stringify!(#typ),
                            self.#name()#maybe_unwrap,
                            self.offset_data(),
                        )
                ))
            }

            FieldType::Offset {
                target: Some(target),
                typ,
            } => {
                let array_type = format!("{typ}({target})");
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
                        FieldType::offset_array(
                            #array_type,
                            self.#name()#maybe_unwrap,
                            move |off| {
                                let target = off.get().#resolve;
                                FieldType::offset(off.get(), target)
                            }
                        ))
                }}
            }
            FieldType::Offset { .. } => quote!(Field::new(#name_str, self.#name()#maybe_unwrap)),
            _ => quote!(compile_error!("unhandled traversal case")),
        },
        FieldType::ComputedArray(arr) => {
            // if we are in a record, we pass in empty data. This lets us
            // avoid a special case in the single instance where this happens, in
            // Class1Record
            let data = in_record
                .then(|| quote!(FontData::new(&[])))
                .unwrap_or_else(|| quote!(self.offset_data()));
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
        FieldType::Other { typ } if typ.is_ident("ValueRecord") => {
            let clone = in_record.then(|| quote!(.clone()));
            quote!(Field::new(#name_str, self.#name() #clone #maybe_unwrap))
        }
        FieldType::Other { .. } => {
            quote!(compile_error!(concat!("another weird type: ", #name_str)))
        }
    }
}

impl Field {
    pub(crate) fn type_for_record(&self) -> TokenStream {
        match &self.typ {
            FieldType::Offset { typ, .. } if self.is_nullable() => {
                quote!(BigEndian<Nullable<#typ>>)
            }
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => quote!(BigEndian<#typ>),
            FieldType::Other { typ } => typ.to_token_stream(),
            FieldType::ComputedArray(array) => {
                let inner = array.type_with_lifetime();
                quote!(ComputedArray<'a, #inner>)
            }
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ, .. } if self.is_nullable() => {
                    quote!(&'a [BigEndian<Nullable<#typ>>])
                }
                FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
                    quote!(&'a [BigEndian<#typ>])
                }
                FieldType::Other { typ } => quote!( &[#typ] ),
                _ => unreachable!("no nested arrays"),
            },
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

    pub(crate) fn has_computed_len(&self) -> bool {
        self.attrs.len.is_some()
            || self.attrs.count.is_some()
            || self.attrs.read_with_args.is_some()
    }

    pub(crate) fn is_version_dependent(&self) -> bool {
        self.attrs.available.is_some()
    }

    /// Ensure attributes are sane; this is run after parsing, so we can report
    /// any errors in a reasonable way.
    fn sanity_check(&self) -> syn::Result<()> {
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
        }
        if self.is_array() && (self.attrs.count.is_none() && self.attrs.len.is_none()) {
            return Err(syn::Error::new(
                self.name.span(),
                "array requires #[count] attribute",
            ));
        }
        if let Some(args) = &self.attrs.read_with_args {
            if let Some(len) = &self.attrs.len {
                return Err(syn::Error::new(
                    len.span(),
                    "#[len_expr] unnecessary, #[read_with] provides computed length",
                ));
            }
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
            FieldType::Other { .. } | FieldType::Array { .. } | FieldType::ComputedArray { .. } => {
                let len_field = self.shape_byte_len_field_name();
                let try_op = self.is_version_dependent().then(|| quote!(?));
                quote!(self.#len_field #try_op)
            }
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
            .chain(self.attrs.len.as_ref().into_iter().flat_map(|expr| {
                expr.referenced_fields
                    .iter()
                    .map(|x| (x.clone(), NeededWhen::Parse))
            }))
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
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => typ.to_token_stream(),
            FieldType::Other { typ } => typ.to_token_stream(),
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ, .. } if self.is_nullable() => {
                    quote!(&'a [BigEndian<Nullable<#typ>>])
                }
                FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
                    quote!(&'a [BigEndian<#typ>])
                }
                FieldType::Other { typ } => quote!( &'a [#typ] ),
                _ => unreachable!(),
            },
            FieldType::ComputedArray(array) => {
                let inner = array.type_with_lifetime();
                quote!(ComputedArray<'a, #inner>)
            }
        }
    }

    pub(crate) fn owned_type(&self) -> TokenStream {
        if let Some(typ) = &self.attrs.compile_type {
            return typ.into_token_stream();
        }
        self.typ.compile_type(self.is_nullable())
    }

    pub(crate) fn table_getter(&self, generic: Option<&syn::Ident>) -> Option<TokenStream> {
        if !self.has_getter() {
            return None;
        }

        let name = &self.name;
        let is_array = self.is_array();
        let is_versioned = self.is_version_dependent();

        let mut return_type = self.raw_getter_return_type();
        if is_versioned {
            return_type = quote!(Option<#return_type>);
        }

        let range_stmt = self.getter_range_stmt();
        let mut read_stmt = if let Some(args) = &self.attrs.read_with_args {
            let get_args = args.to_tokens_for_table_getter();
            quote!( self.data.read_with_args(range, &#get_args).unwrap() )
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
            FieldType::Other { .. } | FieldType::ComputedArray { .. }
        )
        .then(|| quote!(&));

        let getter_expr = match &self.typ {
            FieldType::Scalar { .. } | FieldType::Offset { .. } => quote!(self.#name.get()),
            FieldType::Other { .. } | FieldType::ComputedArray { .. } => quote!(&self.#name),
            FieldType::Array { .. } => quote!(self.#name),
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
            _ if self.attrs.skip_offset_getter.is_some() => return None,
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
        let target_is_generic = Some(target) == generic;
        let add_lifetime = (!target_is_generic).then(|| quote!(<'a>));
        let where_read_clause = target_is_generic.then(|| quote!(where T: FontRead<'a>));
        let mut return_type = quote!(Result<#target #add_lifetime, ReadError>);

        if self.is_nullable() || self.attrs.available.is_some() {
            return_type = quote!(Option<#return_type>);
        }
        if self.is_array() {
            return_type = quote!(impl Iterator<Item=#return_type> + 'a);
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

        // if this is version dependent we append ? when we call the offset getter
        let try_op = self.is_version_dependent().then(|| quote!(?));

        let docs = format!(" Attempt to resolve [`{raw_name}`][Self::{raw_name}].");

        if self.is_array() {
            let name = &self.name;
            Some(quote! {
                pub fn #getter_name #decl_lifetime_if_needed (&self #input_data_if_needed) -> #return_type #where_read_clause {
                    #data_alias_if_needed
                    #args_if_needed
                    self.#name().iter().map(move |off| off.get().#resolve)
                }
            })
        } else {
            Some(quote! {
                #[doc = #docs]
                pub fn #getter_name #decl_lifetime_if_needed (&self #input_data_if_needed) -> #return_type #where_read_clause {
                    #data_alias_if_needed
                    #args_if_needed
                    self.#raw_name() #try_op .#resolve
                }
            })
        }
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
        let name_string = self.name.to_string();
        let offset_name = name_string
            .trim_end_matches("_offsets")
            .trim_end_matches("_offset");
        Some(syn::Ident::new(offset_name, self.name.span()))
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

        if let FieldType::Other { typ } = &self.typ {
            return Some(quote!( <#typ as ComputeSize>::compute_size(&#read_args)));
        }

        let len_expr = if let Some(expr) = &self.attrs.len {
            expr.expr.to_token_stream()
        } else if let Some(Count::All) = self.attrs.count.as_deref() {
            quote!(cursor.remaining_bytes())
        } else {
            let count_expr = self
                .attrs
                .count
                .as_deref()
                .map(Count::count_expr)
                .expect("must have one of count/count_expr/len");
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

    fn compile_write_stmt(&self) -> TokenStream {
        let value_expr = if let Some(format) = &self.attrs.format {
            let typ = self.typ.cooked_type_tokens();
            quote!( (#format as #typ) )
        } else if let Some(computed) = &self.attrs.compile {
            let typ = self.typ.cooked_type_tokens();
            let expr = computed.compile_expr();
            if !computed.referenced_fields.is_empty() {
                quote!( (#expr.unwrap() as #typ) )
            } else {
                quote!( (#expr as #typ) )
            }
            // not computed
        } else {
            let name = &self.name;
            quote!( self.#name )
        };

        quote!(#value_expr.write_into(writer))
    }

    fn compile_write_contains_int_cast(&self) -> bool {
        self.attrs.format.is_some() || self.attrs.compile.is_some()
    }

    pub(crate) fn gets_recursive_validation(&self) -> bool {
        match &self.typ {
            FieldType::Scalar { .. } | FieldType::Other { .. } => false,
            FieldType::Offset { target: None, .. } => false,
            FieldType::Offset { .. } | FieldType::ComputedArray { .. } => true,
            FieldType::Array { inner_typ } => matches!(
                inner_typ.as_ref(),
                FieldType::Offset { .. } | FieldType::Other { .. }
            ),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_obj_requires_offset_data(&self, in_record: bool) -> bool {
        match &self.typ {
            FieldType::Offset { .. } => in_record,
            FieldType::ComputedArray(_) => true,
            FieldType::Other { .. } => true,
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { .. } => in_record,
                FieldType::Other { .. } => true,
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
            FieldType::Other { .. } => quote!(obj.#name().to_owned_obj(offset_data)),
            FieldType::Offset {
                target: Some(_), ..
            } => {
                let offset_getter = self.offset_getter_name().unwrap();
                quote!(obj.#offset_getter(#pass_offset_data).into())
            }
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Scalar { .. } => quote!(obj.#name().iter().map(|x| x.get()).collect()),
                FieldType::Offset { .. } => {
                    let offset_getter = self.offset_getter_name().unwrap();
                    quote!(obj.#offset_getter(#pass_offset_data).map(|x| x.into()).collect())
                }
                FieldType::Other { .. } => {
                    quote!(obj.#name().iter().map(|x| FromObjRef::from_obj_ref(x, offset_data)).collect())
                }
                _ => quote!(compile_error!("requires custom to_owned impl")),
            },
            FieldType::ComputedArray(_array) => {
                quote!(obj.#name().iter().filter_map(|x| x.map(|x| FromObjRef::from_obj_ref(&x, offset_data)).ok()).collect())
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
            FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => typ,
            FieldType::Other { typ } => typ
                .get_ident()
                .expect("non-trivial custom types never cooked"),
            FieldType::Array { .. } | FieldType::ComputedArray { .. } => {
                panic!("array tokens never cooked")
            }
        }
    }

    fn compile_type(&self, nullable: bool) -> TokenStream {
        match self {
            FieldType::Scalar { typ } => typ.into_token_stream(),
            FieldType::Other { typ } => typ.into_token_stream(),
            FieldType::Offset { typ, target } => {
                let target = target
                    .as_ref()
                    .map(|t| t.into_token_stream())
                    .unwrap_or_else(|| quote!(Box<dyn FontWrite>));
                let width = width_for_offset(typ);
                if nullable {
                    quote!(NullableOffsetMarker<#target, #width>)
                } else {
                    quote!(OffsetMarker<#target, #width>)
                }
            }
            FieldType::Array { inner_typ } => {
                if matches!(inner_typ.as_ref(), &FieldType::Array { .. }) {
                    panic!("nesting arrays is not supported");
                }

                let inner_tokens = inner_typ.compile_type(nullable);
                quote!( Vec<#inner_tokens> )
            }
            FieldType::ComputedArray(array) => array.compile_type(),
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
