//! Codegen for the reworked parsing framework.
//!
//! Emits into a parallel tree (`read_fonts::exp::tables`) rather than replacing
//! the existing output, so this can be developed against real tables without
//! breaking anything. See `docs/parsing-rework.md`.
//!
//! Only the read side is emitted: no compile side, and no traversal.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use crate::{
    flags_enums,
    parsing::{
        Condition, Count, CountArg, Field, FieldType, GenericGroup, Item, Items, OffsetTarget,
        Record, Table, TableFormat,
    },
};

pub(crate) fn generate_module(items: &Items) -> Result<TokenStream, syn::Error> {
    // records that hold an offset are handed out paired with their base; the
    // rest stay plain slices. Which one a field is depends on the record, so
    // the set has to be known before any field is emitted.
    let with_parent: HashSet<syn::Ident> = items
        .iter()
        .filter_map(|item| match item {
            Item::Record(rec) if record_shape(rec) == RecordShape::ZerocopyWithParent => {
                Some(rec.name.clone())
            }
            _ => None,
        })
        .collect();

    let mut code = Vec::new();
    for item in items.iter() {
        let item_code = match item {
            Item::Table(item) => generate_table(item, &with_parent)?,
            Item::Record(item) => generate_record(item, &with_parent)?,
            Item::Format(item) => generate_format_group(item)?,
            // flags and raw enums are pure scalar types, independent of how
            // tables are read, so the existing emission is reused verbatim
            Item::RawEnum(item) => flags_enums::generate_raw_enum(item),
            Item::Flags(item) => flags_enums::generate_flags(item),
            Item::GenericGroup(item) => generate_generic_group(item)?,
            Item::Extern(..) => Default::default(),
        };
        code.push(item_code);
    }

    Ok(quote! {
        #[allow(unused_imports)]
        use crate::exp::prelude::*;
        #(#code)*
    })
}

/// Where a field's bytes are measured from.
///
/// A table's fields are located from byte zero of its own data. A computed
/// record's are located from its position within the parent, which is the only
/// difference between the two emissions.
#[derive(Clone, Copy)]
struct Base {
    /// The expression for the enclosing data.
    data: fn() -> TokenStream,
    /// The expression for the first field's start.
    start: fn() -> TokenStream,
}

const TABLE_BASE: Base = Base {
    data: || quote!(self.data),
    start: || quote!(0),
};

const RECORD_BASE: Base = Base {
    data: || quote!(self.parent),
    start: || quote!(self.pos),
};

// ---------------------------------------------------------------------------
// tables
// ---------------------------------------------------------------------------

fn generate_table(item: &Table, with_parent: &HashSet<syn::Ident>) -> syn::Result<TokenStream> {
    if item.attrs.write_only.is_some() {
        return Ok(Default::default());
    }
    let docs = &item.attrs.docs;
    let name = item.raw_name();
    let generic = item.attrs.generic_offset.as_ref();
    let generic_with_default = generic.map(|t| quote!(#t = ()));
    let phantom_decl = generic.map(|t| quote!(offset_type: core::marker::PhantomData<*const #t>));
    let phantom_init = generic.map(|_| quote!(offset_type: core::marker::PhantomData,));

    let min_size = item.min_valid_size_expr();
    let byte_range_fns = byte_range_fns(&item.fields.fields, TABLE_BASE);
    let getters = item
        .fields
        .iter()
        .filter_map(|fld| {
            getter(
                fld,
                &item.fields.fields,
                TABLE_BASE,
                generic.map(|g| &g.attr),
                with_parent,
            )
        })
        .collect::<Vec<_>>();

    let read_args = item.attrs.read_args.as_ref();
    let args_type = read_args
        .map(|a| a.args_type())
        .unwrap_or_else(|| quote!(()));
    let destructure = read_args.map(|a| a.destructure_pattern());
    let stored_arg_decls = read_args
        .map(|a| a.constructor_args().collect::<Vec<_>>())
        .unwrap_or_default();
    let stored_arg_inits = read_args
        .map(|a| a.idents().map(|id| quote!(#id,)).collect::<Vec<_>>())
        .unwrap_or_default();
    let args_binding = if read_args.is_some() {
        quote!(args)
    } else {
        quote!(_)
    };
    let arg_getters = read_args
        .map(|a| {
            a.args
                .iter()
                .map(|arg| {
                    let ident = &arg.ident;
                    let typ = &arg.typ;
                    quote! {
                        pub fn #ident(&self) -> #typ {
                            self.#ident
                        }
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // tables that take args get a named constructor, since `Table::read` is
    // only available when `Args = ()`
    let named_read_ctor = read_args.map(|a| {
        let ctor_args = a.constructor_args().collect::<Vec<_>>();
        let build = a.read_args_from_constructor_args();
        quote! {
            #[allow(clippy::needless_lifetimes)]
            impl<'a> #name<'a> {
                /// Reads this table, which requires external state.
                pub fn read(data: Bytes<'a>, #( #ctor_args ),*) -> Option<Self> {
                    let args = #build;
                    <Self as Table<'a>>::read_with_args(data, args)
                }
            }
        }
    });

    let of_unit_impl = generic.map(|t| {
        quote! {
            #[allow(clippy::needless_lifetimes)]
            impl<'a, #t> #name<'a, #t> {
                /// This table with its specific generic replaced by `()`.
                pub fn of_unit_type(&self) -> #name<'a, ()> {
                    #name { data: self.data, offset_type: core::marker::PhantomData }
                }
            }
        }
    });
    let format_impl = item.impl_format_trait();
    let discriminant_impl = exp_discriminant_impl(item);
    let sanitize = sanitize_impl(
        &name,
        &item.fields.fields,
        generic.map(|g| &g.attr),
        with_parent,
        TABLE_BASE,
        true,
    );
    let fast_sanitize = fast_sanitize_impl(
        &name,
        &item.fields.fields,
        generic.map(|g| &g.attr),
        with_parent,
        TABLE_BASE,
        true,
    );
    let top_level = item.attrs.tag.as_ref().map(|tag| {
        let tag_str = tag.value();
        let byte_tag = syn::LitByteStr::new(tag_str.as_bytes(), tag.span());
        quote! {
            impl TopLevelTable for #name<'_> {
                const TAG: Tag = Tag::new(#byte_tag);
            }
        }
    });

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Copy)]
        pub struct #name<'a, #generic_with_default> {
            data: Bytes<'a>,
            #( #stored_arg_decls, )*
            #phantom_decl
        }

        impl<'a, #generic> Table<'a> for #name<'a, #generic> {
            type Args = #args_type;

            const MIN_SIZE: usize = #min_size;

            fn read_with_args(data: Bytes<'a>, #args_binding: Self::Args) -> Option<Self> {
                #destructure
                #[allow(clippy::absurd_extreme_comparisons)]
                if data.len() < <Self as Table<'a>>::MIN_SIZE {
                    return None;
                }
                Some(Self { data, #( #stored_arg_inits )* #phantom_init })
            }
        }

        #named_read_ctor
        #top_level
        #format_impl
        #discriminant_impl
        #sanitize
        #fast_sanitize

        #of_unit_impl

        #[allow(clippy::needless_lifetimes)]
        impl<'a, #generic> #name<'a, #generic> {
            /// The data this table's offsets are measured from.
            pub fn offset_data(&self) -> Bytes<'a> {
                self.data
            }

            #( #arg_getters )*
            #( #getters )*
            #( #byte_range_fns )*
        }
    })
}

/// The inline discriminant a generic group reads to pick its payload type.
fn exp_discriminant_impl(item: &Table) -> Option<TokenStream> {
    let field = item
        .fields
        .iter()
        .find(|fld| fld.attrs.discriminant.is_some())?;
    let name = item.raw_name();
    let parts: Vec<_> = item
        .fields
        .iter()
        .take_while(|fld| fld.name != field.name)
        .map(|fld| {
            fld.known_min_size_stmt()
                .expect("all fields before #[discriminant] must have a known size")
        })
        .filter(|t| !t.is_empty())
        .collect();
    let offset = match parts.as_slice() {
        [] => quote!(0),
        [one] => one.to_owned(),
        more => quote!( (#(#more)+*) ),
    };
    Some(quote! {
        impl Discriminant for #name<'_, ()> {
            fn read_discriminant(data: Bytes<'_>) -> Option<u16> {
                data.read_at(#offset)
            }
        }
    })
}

// ---------------------------------------------------------------------------
// generic groups
// ---------------------------------------------------------------------------

/// An enum over the payload types a wrapper can hold, chosen by a discriminant
/// in the wrapper. GPOS/GSUB lookups.
fn generate_generic_group(item: &GenericGroup) -> syn::Result<TokenStream> {
    let docs = &item.attrs.docs;
    let name = &item.name;
    let inner = &item.inner_type;
    let variants = item.variants.iter().map(|v| {
        let vname = &v.name;
        let typ = &v.typ;
        quote!( #vname(#inner<'a, #typ<'a>>), )
    });
    let arms = item.variants.iter().map(|v| {
        let vname = &v.name;
        let type_id = &v.type_id;
        quote!( #type_id => Some(#name::#vname(<#inner<'a, _> as Table<'a>>::read(data)?)), )
    });
    let of_unit_arms = item.variants.iter().map(|v| {
        let vname = &v.name;
        quote!( #name::#vname(inner) => inner.of_unit_type(), )
    });
    let group_name_str = name.to_string();
    let group_sanitize_arms = item
        .variants
        .iter()
        .map(|v| {
            let vname = &v.name;
            quote!( Self::#vname(inner) => inner.sanitize_in(ctx), )
        })
        .collect::<Vec<_>>();
    let group_fast_arms = item
        .variants
        .iter()
        .map(|v| {
            let vname = &v.name;
            quote!( Self::#vname(inner) => inner.fast_sanitize_in(ctx), )
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Copy)]
        pub enum #name<'a> {
            #( #variants )*
        }

        #[cfg(feature = "sanitize")]
        impl<'a> Sanitize<'a> for #name<'a> {
            const TYPE_NAME: &'static str = #group_name_str;

            fn sanitize_in(&self, ctx: &mut SanitizeContext) {
                match self {
                    #( #group_sanitize_arms )*
                }
            }
        }

        #[cfg(feature = "fast_sanitize")]
        impl<'a> FastSanitize<'a> for #name<'a> {
            fn fast_sanitize_in(&self, ctx: &mut FastSanitizeContext) -> bool {
                match self {
                    #( #group_fast_arms )*
                }
            }
        }

        impl<'a> Table<'a> for #name<'a> {
            type Args = ();

            const MIN_SIZE: usize = <#inner<'a, ()> as Table<'a>>::MIN_SIZE;

            fn read_with_args(data: Bytes<'a>, _: ()) -> Option<Self> {
                match <#inner<'_, ()> as Discriminant>::read_discriminant(data)? {
                    #( #arms )*
                    _ => None,
                }
            }
        }

        #[allow(clippy::needless_lifetimes)]
        impl<'a> #name<'a> {
            /// The inner table with its specific generic erased, so that one
            /// concrete type carries the methods.
            pub fn of_unit_type(&self) -> #inner<'a, ()> {
                match self {
                    #( #of_unit_arms )*
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// records
// ---------------------------------------------------------------------------

/// Which of the shapes a record takes.
///
/// See `docs/parsing-rework.md`. The choice is made by how the record's byte
/// length is known, plus whether it holds an offset.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RecordShape {
    /// Fixed size, no offsets: a plain zerocopy struct, handed out as `&'a [R]`.
    Zerocopy,
    /// Fixed size, holds an offset: zerocopy, reached through `WithParent`.
    ZerocopyWithParent,
    /// Size computed from read args: a cursor into the parent.
    Computed,
}

fn record_shape(item: &Record) -> RecordShape {
    let computed =
        item.attrs.read_args.is_some() || item.fields.iter().any(|f| f.has_computed_len());
    if computed {
        RecordShape::Computed
    } else if item.fields.iter().any(is_offset_field) {
        RecordShape::ZerocopyWithParent
    } else {
        RecordShape::Zerocopy
    }
}

fn generate_record(item: &Record, with_parent: &HashSet<syn::Ident>) -> syn::Result<TokenStream> {
    match record_shape(item) {
        RecordShape::Zerocopy => generate_zerocopy_record(item, false),
        RecordShape::ZerocopyWithParent => generate_zerocopy_record(item, true),
        RecordShape::Computed => generate_computed_record(item, with_parent),
    }
}

/// A fixed-size record: the same zerocopy struct emitted today, except that any
/// offset accessors move onto `WithParent`, where the base is already held.
fn generate_zerocopy_record(item: &Record, with_parent: bool) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let field_decls = item.fields.iter().map(|fld| {
        let fname = &fld.name;
        let fdocs = &fld.attrs.docs;
        let typ = zerocopy_field_type(fld);
        quote!( #( #fdocs )* pub #fname: #typ, )
    });
    let plain_getters = item.fields.iter().filter_map(zerocopy_plain_getter);
    let raw_byte_len = item
        .fields
        .iter()
        .map(|fld| {
            let typ = fld.typ.cooked_type_tokens();
            quote!(#typ::RAW_BYTE_LEN)
        })
        .collect::<Vec<_>>();

    let with_parent_impl = with_parent.then(|| {
        let offset_getters = item
            .fields
            .iter()
            .filter_map(with_parent_offset_getter)
            .collect::<Vec<_>>();
        let checks = item.fields.iter().filter_map(|fld| {
            let FieldType::Offset { .. } = &fld.typ else {
                return None;
            };
            if fld.attrs.offset_getter.is_some() {
                return None;
            }
            let getter = fld.offset_getter_name()?;
            let raw = &fld.name;
            let raw_str = raw.to_string();
            let nullable = is_nullable(fld);
            let raw_value = if nullable {
                quote!( self.#raw().offset().to_u32() )
            } else {
                quote!( self.#raw().to_u32() )
            };
            Some(quote! {
                {
                    let target = self.#getter();
                    ctx.check_offset(#raw_str, #raw_value, target.is_some(), #nullable);
                    if let Some(target) = target {
                        ctx.enter_field(#raw_str);
                        target.sanitize_in(ctx);
                        ctx.exit_field();
                    }
                }
            })
        });
        let type_name = name.to_string();
        let checks = checks.collect::<Vec<_>>();
        let fast_checks = item
            .fields
            .iter()
            .filter_map(|fld| {
                let FieldType::Offset { .. } = &fld.typ else {
                    return None;
                };
                if fld.attrs.offset_getter.is_some() {
                    return None;
                }
                let getter = fld.offset_getter_name()?;
                let raw = &fld.name;
                let nullable = is_nullable(fld);
                let raw_value = if nullable {
                    quote!( self.#raw().offset().to_u32() )
                } else {
                    quote!( self.#raw().to_u32() )
                };
                // a nullable offset may legitimately be zero; one that is not, may not
                let null_case = if nullable {
                    quote!( if #raw_value == 0 { return true; } )
                } else {
                    quote!()
                };
                Some(quote! {
                    {
                        #null_case
                        let Some(target) = self.#getter() else {
                            return false;
                        };
                        if !target.fast_sanitize_in(ctx) {
                            return false;
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        let fast_ctx_param = if fast_checks.is_empty() {
            quote!(_ctx)
        } else {
            quote!(ctx)
        };
        // a record whose offsets all have hand-written resolvers has nothing
        // here: what those resolvers reach is outside what the pass can see
        let ctx_param = if checks.is_empty() {
            quote!(_ctx)
        } else {
            quote!(ctx)
        };
        quote! {
            #[allow(clippy::needless_lifetimes)]
            impl<'a> WithParent<'a, #name> {
                #( #offset_getters )*
            }

            #[cfg(feature = "fast_sanitize")]
            impl<'a> FastSanitize<'a> for WithParent<'a, #name> {
                fn fast_sanitize_in(&self, #fast_ctx_param: &mut FastSanitizeContext) -> bool {
                    #( #fast_checks )*
                    true
                }
            }

            #[cfg(feature = "sanitize")]
            impl<'a> Sanitize<'a> for WithParent<'a, #name> {
                const TYPE_NAME: &'static str = #type_name;

                fn sanitize_in(&self, #ctx_param: &mut SanitizeContext) {
                    // a record adds no step of its own: the path already says
                    // which field and which element we are in, and a record has
                    // no identity to guard against revisiting — its extent was
                    // checked by whoever located the run
                    #( #checks )*
                }
            }
        }
    });

    Ok(quote! {
        #( #docs )*
        // `Default` is what lets a caller reaching past the end of a slice say
        // `.copied().unwrap_or_default()`: every field is a scalar, and an
        // offset defaults to null
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::AnyBitPattern)]
        #[repr(C, packed)]
        pub struct #name {
            #( #field_decls )*
        }

        impl FixedSize for #name {
            const RAW_BYTE_LEN: usize = #( #raw_byte_len )+*;
        }

        impl #name {
            #( #plain_getters )*
        }

        #with_parent_impl
    })
}

/// A record whose size is computed from its read args: a cursor holding the
/// parent, its own position within it, and the args.
fn generate_computed_record(
    item: &Record,
    with_parent: &HashSet<syn::Ident>,
) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let read_args = item.attrs.read_args.as_ref();
    let args_type = read_args
        .map(|a| a.args_type())
        .unwrap_or_else(|| quote!(()));
    let destructure = read_args.map(|a| a.destructure_pattern());
    let arg_getters = read_args
        .map(|a| {
            a.args
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    let ident = &arg.ident;
                    let typ = &arg.typ;
                    let access = if a.args.len() == 1 {
                        quote!(self.args)
                    } else {
                        let idx = syn::Index::from(i);
                        quote!(self.args.#idx)
                    };
                    quote! {
                        pub fn #ident(&self) -> #typ {
                            #access
                        }
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // a record whose length the font declares uses that, rather than the sum of
    // its fields: the declared size may be larger than what the fields occupy
    let size_body = match item.attrs.record_size.as_ref() {
        Some(arg) => {
            let arg = &arg.attr;
            quote!( #arg as usize )
        }
        None => {
            let terms = item
                .fields
                .iter()
                .map(|fld| len_expr(fld, None))
                .collect::<Vec<_>>();
            quote!( 0usize #( .saturating_add(#terms) )* )
        }
    };
    let byte_range_fns = byte_range_fns(&item.fields.fields, RECORD_BASE);
    let sanitize = sanitize_impl(
        name,
        &item.fields.fields,
        None,
        with_parent,
        RECORD_BASE,
        false,
    );
    let fast_sanitize = fast_sanitize_impl(
        name,
        &item.fields.fields,
        None,
        with_parent,
        RECORD_BASE,
        false,
    );
    let getters = item
        .fields
        .iter()
        .filter_map(|fld| getter(fld, &item.fields.fields, RECORD_BASE, None, with_parent))
        .collect::<Vec<_>>();

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Copy)]
        pub struct #name<'a> {
            /// The enclosing table's data: what this record's offsets are
            /// measured from.
            parent: Bytes<'a>,
            /// This record's position within `parent`.
            pos: usize,
            args: #args_type,
        }

        impl ComputedSize for #name<'_> {
            type Args = #args_type;

            #[allow(unused_variables)]
            fn computed_size(args: #args_type) -> usize {
                #destructure
                let _ = &args;
                #size_body
            }
        }

        #[allow(clippy::needless_lifetimes)]
        impl<'a> #name<'a> {
            /// Locates the record at `pos` bytes into `parent`.
            ///
            /// Performs no reads.
            #[inline]
            pub fn at(parent: Bytes<'a>, pos: usize, args: #args_type) -> Self {
                Self { parent, pos, args }
            }

            /// The data this record's offsets are measured from.
            pub fn offset_data(&self) -> Bytes<'a> {
                self.parent
            }

            #( #arg_getters )*
            #( #getters )*
            #( #byte_range_fns )*
        }

        impl<'a> ArrayElement<'a> for #name<'a> {
            type Args = #args_type;
            type Store = StridedStore<'a>;
            type Output = Self;

            #[inline]
            fn read(store: StridedStore<'a>, item: usize, args: #args_type) -> Self {
                Self::at(store.data(), item, args)
            }
        }

        #sanitize
        #fast_sanitize
    })
}

// ---------------------------------------------------------------------------
// format groups
// ---------------------------------------------------------------------------

fn generate_format_group(item: &TableFormat) -> syn::Result<TokenStream> {
    let name = &item.name;
    let docs = &item.attrs.docs;
    let live = || {
        item.variants
            .iter()
            .filter(|v| v.attrs.write_only.is_none())
    };
    let variants = live().map(|v| {
        let vname = &v.name;
        let typ = v.type_name();
        let vdocs = &v.attrs.docs;
        quote!( #( #vdocs )* #vname(#typ<'a>), )
    });
    let mut has_match_stmt = false;
    let match_arms = live()
        .map(|v| {
            let vname = &v.name;
            let typ = v.type_name();
            let lhs = if let Some(expr) = v.attrs.match_stmt.as_deref() {
                has_match_stmt = true;
                let expr = &expr.expr;
                quote!(format if #expr)
            } else {
                quote!(#typ::FORMAT)
            };
            quote!( #lhs => Some(Self::#vname(<#typ as Table<'a>>::read(data)?)), )
        })
        .collect::<Vec<_>>();
    let offset_data_arms = live().map(|v| {
        let vname = &v.name;
        quote!( Self::#vname(item) => item.offset_data(), )
    });
    let min_size_terms = live()
        .map(|v| {
            let typ = v.type_name();
            quote!( <#typ as Table>::MIN_SIZE )
        })
        .collect::<Vec<_>>();
    let name_str = name.to_string();
    let sanitize_arms = live()
        .map(|v| {
            let vname = &v.name;
            quote!( Self::#vname(item) => item.sanitize_in(ctx), )
        })
        .collect::<Vec<_>>();
    let fast_arms = live()
        .map(|v| {
            let vname = &v.name;
            quote!( Self::#vname(item) => item.fast_sanitize_in(ctx), )
        })
        .collect::<Vec<_>>();
    let format_typ = &item.format;
    let format_offset = item
        .format_offset
        .as_ref()
        .map(|lit| lit.base10_parse::<usize>().unwrap_or(0))
        .unwrap_or(0);
    let maybe_allow_lint = has_match_stmt.then(|| quote!(#[allow(clippy::redundant_guards)]));

    Ok(quote! {
        #( #docs )*
        #[derive(Clone, Copy)]
        pub enum #name<'a> {
            #( #variants )*
        }

        #[allow(clippy::needless_lifetimes)]
        impl<'a> #name<'a> {
            /// The data this table's offsets are measured from.
            pub fn offset_data(&self) -> Bytes<'a> {
                match self {
                    #( #offset_data_arms )*
                }
            }
        }

        #[cfg(feature = "sanitize")]
        impl<'a> Sanitize<'a> for #name<'a> {
            const TYPE_NAME: &'static str = #name_str;

            fn sanitize_in(&self, ctx: &mut SanitizeContext) {
                match self {
                    #( #sanitize_arms )*
                }
            }
        }

        #[cfg(feature = "fast_sanitize")]
        impl<'a> FastSanitize<'a> for #name<'a> {
            fn fast_sanitize_in(&self, ctx: &mut FastSanitizeContext) -> bool {
                match self {
                    #( #fast_arms )*
                }
            }
        }

        impl<'a> Table<'a> for #name<'a> {
            type Args = ();

            /// The smallest of the variants: which one is present is not known
            /// until the format word is read.
            const MIN_SIZE: usize = {
                let sizes = [ #( #min_size_terms, )* ];
                let mut min = usize::MAX;
                let mut i = 0;
                while i < sizes.len() {
                    if sizes[i] < min {
                        min = sizes[i];
                    }
                    i += 1;
                }
                min
            };

            #maybe_allow_lint
            fn read_with_args(data: Bytes<'a>, _: ()) -> Option<Self> {
                let format: #format_typ = data.read_at(#format_offset)?;
                match format {
                    #( #match_arms )*
                    _ => None,
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// fields
// ---------------------------------------------------------------------------

fn is_offset_field(fld: &Field) -> bool {
    match &fld.typ {
        FieldType::Offset { .. } => true,
        FieldType::Array { inner_typ } => matches!(inner_typ.as_ref(), FieldType::Offset { .. }),
        _ => false,
    }
}

fn is_nullable(fld: &Field) -> bool {
    fld.attrs.nullable.is_some()
}

/// The declared type of a field inside a zerocopy record.
fn zerocopy_field_type(fld: &Field) -> TokenStream {
    match &fld.typ {
        FieldType::Offset { typ, .. } if is_nullable(fld) => quote!(BigEndian<Nullable<#typ>>),
        FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
            if typ == "u8" {
                quote!(u8)
            } else {
                quote!(BigEndian<#typ>)
            }
        }
        FieldType::Struct { typ } => quote!(#typ),
        other => panic!("zerocopy record cannot hold {other:?}"),
    }
}

fn zerocopy_plain_getter(fld: &Field) -> Option<TokenStream> {
    if !fld.has_getter() {
        return None;
    }
    let name = &fld.name;
    let docs = &fld.attrs.docs;
    let (typ, body) = match &fld.typ {
        FieldType::Offset { typ, .. } if is_nullable(fld) => {
            (quote!(Nullable<#typ>), quote!(self.#name.get()))
        }
        FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
            if typ == "u8" {
                (quote!(u8), quote!(self.#name))
            } else {
                (quote!(#typ), quote!(self.#name.get()))
            }
        }
        FieldType::Struct { typ } => (quote!(&#typ), quote!(&self.#name)),
        other => panic!("zerocopy record cannot hold {other:?}"),
    };
    Some(quote! {
        #( #docs )*
        pub fn #name(&self) -> #typ {
            #body
        }
    })
}

/// The offset accessor for a zerocopy record, emitted on `WithParent` so that
/// it needs no `data` argument.
fn with_parent_offset_getter(fld: &Field) -> Option<TokenStream> {
    if fld.attrs.offset_getter.is_some() {
        return None;
    }
    let (_, target) = match &fld.typ {
        FieldType::Offset { typ, target } => (typ, target),
        _ => return None,
    };
    let raw_name = &fld.name;
    let getter_name = fld.offset_getter_name()?;
    let return_type = offset_target_type(target, false);
    let args = fld
        .attrs
        .read_offset_args
        .as_ref()
        .map(|a| a.to_tokens_for_table_getter());
    let resolve = match args {
        Some(args) => quote!(resolve_with_args(self.#raw_name(), #args)),
        None => quote!(resolve(self.#raw_name())),
    };
    let doc = format!(" Resolve [`{raw_name}`][Self::{raw_name}].");
    Some(quote! {
        #[doc = #doc]
        pub fn #getter_name(&self) -> Option<#return_type> {
            self.parent().#resolve
        }
    })
}

/// The type an offset resolves to.
fn offset_target_type(target: &OffsetTarget, is_generic: bool) -> TokenStream {
    match target {
        OffsetTarget::Table(ident) if is_generic => ident.to_token_stream(),
        OffsetTarget::Table(ident) => quote!(#ident<'a>),
        OffsetTarget::Array(inner) => {
            let inner = match inner.as_ref() {
                FieldType::Scalar { typ } => quote!(BigEndian<#typ>),
                FieldType::Struct { typ } => quote!(#typ),
                other => panic!("unexpected offset array target {other:?}"),
            };
            quote!(&'a [#inner])
        }
    }
}

/// The byte length of one field.
///
/// `base` is `None` when emitting inside `ComputedSize::computed_size`, which
/// has no `self`.
fn len_expr(fld: &Field, base: Option<Base>) -> TokenStream {
    if !fld.has_computed_len() {
        let typ = fld.typ.cooked_type_tokens();
        return quote!(#typ::RAW_BYTE_LEN);
    }
    let read_args = fld.attrs.read_with_args.as_deref().map(|a| {
        if base.is_some() {
            a.to_tokens_for_table_getter()
        } else {
            a.to_tokens_for_validation()
        }
    });

    if let FieldType::Struct { typ } = &fld.typ {
        return quote!( <#typ as ComputedSize>::computed_size(#read_args) );
    }

    match fld.attrs.count.as_deref() {
        Some(Count::All(_)) => {
            let data = base
                .map(|b| (b.data)())
                .unwrap_or_else(|| quote!(self.data));
            match &fld.typ {
                FieldType::Array { inner_typ } => {
                    let inner = inner_typ.cooked_type_tokens();
                    quote!(#data.len().saturating_sub(start) / #inner::RAW_BYTE_LEN * #inner::RAW_BYTE_LEN)
                }
                _ => quote!(#data.len().saturating_sub(start)),
            }
        }
        Some(other) => {
            let count_expr = other.count_expr();
            let size_expr = match &fld.typ {
                FieldType::Array { inner_typ } => {
                    let inner = inner_typ.cooked_type_tokens();
                    quote!( #inner::RAW_BYTE_LEN )
                }
                FieldType::ComputedArray(array) => {
                    let inner = array.raw_inner_type();
                    quote!( <#inner as ComputedSize>::computed_size(#read_args) )
                }
                FieldType::VarLenArray(_) => {
                    // a var-len array's extent can only be found by walking it,
                    // which the store does; nothing follows it in a table
                    return quote!(0);
                }
                _ => unreachable!("count not valid here"),
            };
            match other {
                Count::SingleArg(CountArg::Literal(lit)) if lit.base10_digits() == "1" => size_expr,
                _ => quote!( (#count_expr).saturating_mul(#size_expr) ),
            }
        }
        None => quote!(compile_error!("missing count attribute?")),
    }
}

/// `let foo = self.foo();` for each field a `#[count(..)]` expression names.
fn count_arg_decls(fld: &Field, fields: &[Field]) -> Vec<TokenStream> {
    fld.count_arg_names()
        .map(|name| {
            let is_opt = fields
                .iter()
                .find(|f| &f.name == name)
                .map(|f| f.is_conditional())
                .unwrap_or(false);
            let unwrap = is_opt.then(|| quote!(.unwrap_or_default()));
            quote!(let #name = self.#name() #unwrap;)
        })
        .collect()
}

/// The `*_byte_range` accessors, which locate each field in turn.
fn byte_range_fns(fields: &[Field], base: Base) -> Vec<TokenStream> {
    let mut prev_end = (base.start)();
    let mut out = Vec::new();
    for fld in fields {
        let fn_name = fld.shape_byte_range_fn_name();
        let len = len_expr(fld, Some(base));
        let required_decls = count_arg_decls(fld, fields);
        // a field may start where another field's offset points, rather than
        // after the preceding field; the ones after it then follow on from
        // there, so several arrays can share one offset
        if let Some(at) = fld.attrs.at_offset.as_ref() {
            let offset_fld = &at.attr;
            prev_end = quote!( self.#offset_fld().to_u32() as usize );
        }
        let end = match fld.attrs.conditional.as_deref() {
            // the field is there if the record's declared size leaves room for
            // it, which is a statement about where the field ends rather than
            // about any other field's value
            Some(Condition::IfFits) => quote! {
                if start + #len <= self.pos + <Self as ComputedSize>::computed_size(self.args) {
                    start + #len
                } else {
                    start
                }
            },
            Some(cond) => {
                let cond = cond.condition_tokens_for_read();
                quote!( if #cond { start + #len } else { start } )
            }
            None => quote!( start + #len ),
        };
        out.push(quote! {
            pub fn #fn_name(&self) -> Range<usize> {
                #( #required_decls )*
                let start = #prev_end;
                let end = #end;
                start..end
            }
        });
        prev_end = quote!( self.#fn_name().end );
    }
    out
}

/// A field accessor, plus the resolved accessor if the field is an offset.
///
/// The three shapes, which preserve exactly what the crate does today:
///
/// - a field covered by `MIN_SIZE` is guaranteed present, so it is returned
///   bare and the read unwraps;
/// - a non-conditional field beyond `MIN_SIZE` is also returned bare, reading
///   as empty or zero when its extent is not there (`unwrap_or_default`);
/// - a conditional field returns `Option`, because it may legitimately be
///   absent.
///
/// Only offset *resolution* adds an `Option` of its own, since an offset can be
/// null or unreadable whatever the field's shape.
fn getter(
    fld: &Field,
    fields: &[Field],
    base: Base,
    generic: Option<&syn::Ident>,
    with_parent: &HashSet<syn::Ident>,
) -> Option<TokenStream> {
    if !fld.has_getter() {
        return None;
    }
    let name = &fld.name;
    let docs = &fld.attrs.docs;
    let data = (base.data)();
    let range_fn = fld.shape_byte_range_fn_name();
    let is_conditional = fld.is_conditional();
    // guaranteed present by the MIN_SIZE check performed when the table was read
    let guaranteed = fld.validated_at_parse && !is_conditional;
    let count_decls = count_arg_decls(fld, fields);

    let (mut return_type, mut read_stmt) = match &fld.typ {
        FieldType::Array { inner_typ } if matches!(inner_typ.as_ref(), FieldType::Struct { typ } if with_parent.contains(typ)) =>
        {
            let FieldType::Struct { typ } = inner_typ.as_ref() else {
                unreachable!()
            };
            // the raw `&'a [#typ]` is still there, as the array's store
            let count = fld
                .attrs
                .count
                .as_deref()
                .map(|c| c.count_expr())
                .unwrap_or_else(|| quote!(0));
            (
                quote!(Array<'a, WithParent<'a, #typ>>),
                quote!({
                    #( #count_decls )*
                    Array::of_zerocopy_records_or_empty(
                        #data,
                        range.start,
                        transforms::to_usize(#count),
                    )
                }),
            )
        }
        FieldType::Array { .. } => {
            let inner = raw_array_item_type(fld);
            let tail = if guaranteed {
                quote!(.unwrap())
            } else {
                quote!(.unwrap_or_default())
            };
            (
                quote!(&'a [#inner]),
                quote!( #data.read_array(range) #tail ),
            )
        }
        FieldType::ComputedArray(array) => {
            let inner = array.raw_inner_type();
            let args = fld
                .attrs
                .read_with_args
                .as_deref()
                .map(|a| a.to_tokens_for_table_getter())
                .unwrap_or_else(|| quote!(()));
            let count = fld
                .attrs
                .count
                .as_deref()
                .map(|c| c.count_expr())
                .unwrap_or_else(|| quote!(0));
            (
                quote!(Array<'a, #inner<'a>>),
                quote!({
                    #( #count_decls )*
                    Array::of_computed_or_empty(
                        #data,
                        range.start,
                        transforms::to_usize(#count),
                        #args,
                    )
                }),
            )
        }
        FieldType::VarLenArray(array) => {
            let inner = array.raw_inner_type();
            (
                quote!(VariableSizeArray<'a, #inner<'a>>),
                quote!( #data.split_off(range.start).map(VariableSizeArray::of_variable_size).unwrap_or_default() ),
            )
        }
        // an embedded record. one that carries read args has a computed size,
        // so it is a cursor; one that does not is fixed-size and zerocopy, and
        // is paired with its base by `WithParent`. codegen requires the latter
        // to be covered by MIN_SIZE, which is what lets the accessor be
        // non-optional
        FieldType::Struct { typ } => match fld.attrs.read_with_args.as_deref() {
            Some(args) => {
                let args = args.to_tokens_for_table_getter();
                (
                    quote!(#typ<'a>),
                    quote!( #typ::at(#data, range.start, #args) ),
                )
            }
            None => (
                quote!(WithParent<'a, #typ>),
                quote!( WithParent::at(#data, range.start).unwrap() ),
            ),
        },
        FieldType::Offset { typ, .. } if is_nullable(fld) => {
            let tail = if guaranteed {
                quote!(.unwrap())
            } else {
                quote!(.unwrap_or_default())
            };
            (
                quote!(Nullable<#typ>),
                quote!( #data.read_at(range.start) #tail ),
            )
        }
        FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
            let tail = if guaranteed {
                quote!(.unwrap())
            } else {
                quote!(.unwrap_or_default())
            };
            (quote!(#typ), quote!( #data.read_at(range.start) #tail ))
        }
        FieldType::PendingResolution { .. } => panic!("should have resolved {fld:?}"),
    };

    if is_conditional {
        // a conditional field may be absent, which is the one case that is an
        // `Option` rather than a default
        let inner = match &fld.typ {
            FieldType::Array { .. } => {
                let item = raw_array_item_type(fld);
                quote!(&'a [#item])
            }
            _ => return_type.clone(),
        };
        return_type = quote!(Option<#inner>);
        read_stmt = match &fld.typ {
            FieldType::Array { .. } => {
                quote!( (!range.is_empty()).then(|| #data.read_array(range)).flatten() )
            }
            FieldType::Struct { typ } => match fld.attrs.read_with_args.as_deref() {
                Some(args) => {
                    let args = args.to_tokens_for_table_getter();
                    quote!( (!range.is_empty()).then(|| #typ::at(#data, range.start, #args)) )
                }
                None => {
                    quote!( (!range.is_empty()).then(|| WithParent::at(#data, range.start)).flatten() )
                }
            },
            _ => quote!( (!range.is_empty()).then(|| #data.read_at(range.start)).flatten() ),
        };
    }

    let offset_getter = table_offset_getter(fld, base, generic);

    Some(quote! {
        #( #docs )*
        pub fn #name(&self) -> #return_type {
            let range = self.#range_fn();
            #read_stmt
        }

        #offset_getter
    })
}

fn raw_array_item_type(fld: &Field) -> TokenStream {
    let FieldType::Array { inner_typ } = &fld.typ else {
        unreachable!()
    };
    match inner_typ.as_ref() {
        FieldType::Offset { typ, .. } if is_nullable(fld) => quote!(BigEndian<Nullable<#typ>>),
        FieldType::Offset { typ, .. } | FieldType::Scalar { typ } => {
            if typ == "u8" {
                quote!(u8)
            } else {
                quote!(BigEndian<#typ>)
            }
        }
        FieldType::Struct { typ } | FieldType::PendingResolution { typ } => quote!(#typ),
        other => unreachable!("an array should never contain {other:?}"),
    }
}

/// The resolved accessor for an offset field on a table or computed record.
fn table_offset_getter(
    fld: &Field,
    base: Base,
    generic: Option<&syn::Ident>,
) -> Option<TokenStream> {
    if fld.attrs.offset_getter.is_some() {
        return None;
    }
    let target = match &fld.typ {
        FieldType::Offset { target, .. } => target,
        FieldType::Array { inner_typ } => match inner_typ.as_ref() {
            FieldType::Offset { target, .. } => target,
            _ => return None,
        },
        _ => return None,
    };
    let raw_name = &fld.name;
    let getter_name = fld.offset_getter_name()?;
    let data = (base.data)();
    let is_generic = matches!(target, OffsetTarget::Table(id) if Some(id) == generic);
    let args = fld
        .attrs
        .read_offset_args
        .as_ref()
        .map(|a| a.to_tokens_for_table_getter());
    let doc = format!(" Resolve [`{raw_name}`][Self::{raw_name}].");

    if fld.is_array() {
        let OffsetTarget::Table(target_ident) = target else {
            panic!("arrays of offsets to arrays are not in the spec");
        };
        let target_lifetime = (!is_generic).then(|| quote!(<'a>));
        let offset_typ = match &fld.typ {
            FieldType::Array { inner_typ } => match inner_typ.as_ref() {
                FieldType::Offset { typ, .. } => typ.clone(),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        let offset_typ = if is_nullable(fld) {
            quote!(Nullable<#offset_typ>)
        } else {
            quote!(#offset_typ)
        };
        let args_token = args.clone().unwrap_or_else(|| quote!(()));
        let where_clause = is_generic.then(|| quote!(where T: Table<'a, Args = ()>));
        if fld.is_conditional() {
            return Some(quote! {
                #[doc = #doc]
                pub fn #getter_name(&self) -> Option<Array<'a, OffsetTo<#target_ident #target_lifetime, #offset_typ>>>
                    #where_clause
                {
                    Some(Array::of_offsets(self.#raw_name()?, #data, #args_token))
                }
            });
        }
        return Some(quote! {
            #[doc = #doc]
            pub fn #getter_name(&self) -> Array<'a, OffsetTo<#target_ident #target_lifetime, #offset_typ>>
                #where_clause
            {
                Array::of_offsets(self.#raw_name(), #data, #args_token)
            }
        });
    }

    let return_type = offset_target_type(target, is_generic);
    let resolve = match args {
        Some(args) => quote!(resolve_with_args(offset, #args)),
        None => quote!(resolve(offset)),
    };
    let bind_offset = if fld.is_conditional() {
        quote!( let offset = self.#raw_name()?; )
    } else {
        quote!( let offset = self.#raw_name(); )
    };
    let where_clause = is_generic.then(|| quote!(where T: Table<'a, Args = ()>));
    Some(quote! {
        #[doc = #doc]
        pub fn #getter_name(&self) -> Option<#return_type> #where_clause {
            #bind_offset
            #data.#resolve
        }
    })
}

// ---------------------------------------------------------------------------
// sanitize
// ---------------------------------------------------------------------------

/// Emits the pass that reports what the accessors stopped reporting.
///
/// Every check comes from something the table already exposes: a field's extent
/// is its generated `*_byte_range`, and whether an offset resolves is what its
/// resolved accessor answers. So this is a mechanical walk of the same field
/// list the accessors come from, and it cannot drift from them.
fn sanitize_impl(
    name: &syn::Ident,
    fields: &[Field],
    generic: Option<&syn::Ident>,
    with_parent: &HashSet<syn::Ident>,
    base: Base,
    is_table: bool,
) -> TokenStream {
    let type_name = name.to_string();
    let data = (base.data)();

    // Checking every field's extent individually is quadratic: a field's
    // `*_byte_range` is defined as the previous field's end, so evaluating all
    // of them walks the chain once per field.
    //
    // Fields are laid out in order, so the last one's end is the largest, and
    // one comparison against it clears every field before it. Only when that
    // fails is it worth finding out which fields are actually short.
    //
    // `#[at_offset]` breaks the ordering — it jumps somewhere else — so the
    // fields are split into runs at each jump and each run is cleared
    // separately.
    let mut runs: Vec<Vec<&Field>> = Vec::new();
    for fld in fields {
        if runs.is_empty() || fld.attrs.at_offset.is_some() {
            runs.push(Vec::new());
        }
        runs.last_mut().expect("just pushed").push(fld);
    }
    let extent_checks = runs.iter().filter_map(|run| {
        let last = run.last()?;
        let last_range = last.shape_byte_range_fn_name();
        let per_field = run.iter().map(|fld| {
            let fname_str = fld.name.to_string();
            let range_fn = fld.shape_byte_range_fn_name();
            quote!( ctx.check_extent(#fname_str, self.#range_fn(), #data); )
        });
        Some(quote! {
            if self.#last_range().end > #data.len() {
                #( #per_field )*
            }
        })
    });

    let mut checks: Vec<TokenStream> = extent_checks.collect();

    for fld in fields {
        if !fld.has_getter() {
            continue;
        }
        let fname = &fld.name;
        let fname_str = fname.to_string();

        // an offset: did it resolve, and what is on the other side?
        if let Some(getter) = fld.offset_getter_name() {
            if fld.attrs.offset_getter.is_some() {
                // hand-written resolver; we cannot know its shape
                continue;
            }
            let nullable = is_nullable(fld);
            if fld.is_array() {
                checks.push(quote! {
                    ctx.enter_field(#fname_str);
                    {
                        let targets = self.#getter();
                        let budget = ctx.element_budget(targets.len());
                        for (i, target) in targets.iter().enumerate().take(budget) {
                            if ctx.is_done() {
                                break;
                            }
                            ctx.enter_index(i);
                            match target {
                                Some(target) => target.sanitize_in(ctx),
                                None => ctx.report(Problem::NullOffset),
                            }
                            ctx.exit_index();
                        }
                    }
                    ctx.exit_field();
                });
            } else if matches!(
                &fld.typ,
                FieldType::Offset {
                    target: OffsetTarget::Table(_),
                    ..
                }
            ) {
                let raw = quote!( self.#fname() );
                let raw = if fld.is_conditional() {
                    quote!( #raw.unwrap_or_default() )
                } else {
                    raw
                };
                let raw = if nullable {
                    quote!( #raw.offset().to_u32() )
                } else {
                    quote!( #raw.to_u32() )
                };
                checks.push(quote! {
                    {
                        let target = self.#getter();
                        ctx.check_offset(#fname_str, #raw, target.is_some(), #nullable);
                        if let Some(target) = target {
                            ctx.enter_field(#fname_str);
                            target.sanitize_in(ctx);
                            ctx.exit_field();
                        }
                    }
                });
            }
            continue;
        }

        // a run of records that can themselves have something wrong
        match &fld.typ {
            FieldType::ComputedArray(_) => {
                checks.push(quote! {
                    ctx.enter_field(#fname_str);
                    {
                        let items = self.#fname();
                        let budget = ctx.element_budget(items.len());
                        for (i, item) in items.iter().enumerate().take(budget) {
                            if ctx.is_done() {
                                break;
                            }
                            ctx.enter_index(i);
                            item.sanitize_in(ctx);
                            ctx.exit_index();
                        }
                    }
                    ctx.exit_field();
                });
            }
            FieldType::Array { inner_typ } if matches!(inner_typ.as_ref(), FieldType::Struct { typ } if with_parent.contains(typ)) =>
            {
                checks.push(quote! {
                    ctx.enter_field(#fname_str);
                    {
                        let items = self.#fname();
                        let budget = ctx.element_budget(items.len());
                        for (i, item) in items.iter().enumerate().take(budget) {
                            if ctx.is_done() {
                                break;
                            }
                            ctx.enter_index(i);
                            item.sanitize_in(ctx);
                            ctx.exit_index();
                        }
                    }
                    ctx.exit_field();
                });
            }
            FieldType::Struct { typ } if with_parent.contains(typ) => {
                checks.push(quote! {
                    ctx.enter_field(#fname_str);
                    self.#fname().sanitize_in(ctx);
                    ctx.exit_field();
                });
            }
            _ => {}
        }
    }

    let where_clause = generic.map(|t| quote!( where #t: Table<'a, Args = ()> + Sanitize<'a> ));
    let ctx_param = if checks.is_empty() {
        quote!(_ctx)
    } else {
        quote!(ctx)
    };
    let body = if is_table {
        quote! {
            if !#ctx_param.enter_table(#type_name, #data) {
                return;
            }
            #( #checks )*
            #ctx_param.exit_table();
        }
    } else {
        // a record adds no step and no node: the path already says which field
        // and element it is, and it cannot be reached through an offset
        quote!( #( #checks )* )
    };
    quote! {
        #[cfg(feature = "sanitize")]
        impl<'a, #generic> Sanitize<'a> for #name<'a, #generic> #where_clause {
            const TYPE_NAME: &'static str = #type_name;

            fn sanitize_in(&self, #ctx_param: &mut SanitizeContext) {
                #body
            }
        }
    }
}

/// Emits the pass that answers yes or no.
///
/// The same checks [`sanitize_impl`] makes, with everything that exists only to
/// explain them removed: no type name, no field names, no path, no report. What
/// is left is a walk that returns `false` at the first thing wrong, and a
/// binary that links none of the literals the other one needs.
fn fast_sanitize_impl(
    name: &syn::Ident,
    fields: &[Field],
    generic: Option<&syn::Ident>,
    with_parent: &HashSet<syn::Ident>,
    base: Base,
    is_table: bool,
) -> TokenStream {
    let data = (base.data)();
    let mut checks = Vec::new();
    // an extent check just returns; only descending into something needs the
    // context, and a record that holds nothing to descend into never touches it
    let mut uses_ctx = is_table;

    // one comparison per run of fields, as in the detailed pass; here there is
    // nothing to report so the per-field breakdown is simply absent
    let mut runs: Vec<Vec<&Field>> = Vec::new();
    for fld in fields {
        if runs.is_empty() || fld.attrs.at_offset.is_some() {
            runs.push(Vec::new());
        }
        runs.last_mut().expect("just pushed").push(fld);
    }
    for run in &runs {
        let Some(last) = run.last() else { continue };
        let range_fn = last.shape_byte_range_fn_name();
        checks.push(quote! {
            if self.#range_fn().end > #data.len() {
                return false;
            }
        });
    }

    for fld in fields {
        if !fld.has_getter() {
            continue;
        }
        let fname = &fld.name;

        if let Some(getter) = fld.offset_getter_name() {
            if fld.attrs.offset_getter.is_some() {
                continue;
            }
            let nullable = is_nullable(fld);
            if fld.is_array() {
                let miss = if nullable {
                    quote!(continue)
                } else {
                    quote!(return false)
                };
                uses_ctx = true;
                checks.push(quote! {
                    {
                        let targets = self.#getter();
                        let budget = ctx.element_budget(targets.len());
                        for target in targets.iter().take(budget) {
                            let Some(target) = target else { #miss };
                            if !target.fast_sanitize_in(ctx) {
                                return false;
                            }
                        }
                    }
                });
            } else if matches!(
                &fld.typ,
                FieldType::Offset {
                    target: OffsetTarget::Table(_),
                    ..
                }
            ) {
                // a nullable offset may be absent; one that is not may not be
                let body = if nullable {
                    quote! {
                        if let Some(target) = self.#getter() {
                            if !target.fast_sanitize_in(ctx) {
                                return false;
                            }
                        }
                    }
                } else {
                    quote! {
                        let Some(target) = self.#getter() else {
                            return false;
                        };
                        if !target.fast_sanitize_in(ctx) {
                            return false;
                        }
                    }
                };
                uses_ctx = true;
                checks.push(quote!( { #body } ));
            }
            continue;
        }

        let descends = matches!(&fld.typ, FieldType::ComputedArray(_))
            || matches!(&fld.typ, FieldType::Struct { typ } if with_parent.contains(typ))
            || matches!(&fld.typ, FieldType::Array { inner_typ }
                if matches!(inner_typ.as_ref(), FieldType::Struct { typ } if with_parent.contains(typ)));
        uses_ctx |= descends;
        match &fld.typ {
            FieldType::ComputedArray(_) => checks.push(quote! {
                {
                    let items = self.#fname();
                    let budget = ctx.element_budget(items.len());
                    for item in items.iter().take(budget) {
                        if !item.fast_sanitize_in(ctx) {
                            return false;
                        }
                    }
                }
            }),
            FieldType::Array { inner_typ } if matches!(inner_typ.as_ref(), FieldType::Struct { typ } if with_parent.contains(typ)) =>
            {
                uses_ctx = true;
                checks.push(quote! {
                    {
                        let items = self.#fname();
                        let budget = ctx.element_budget(items.len());
                        for item in items.iter().take(budget) {
                            if !item.fast_sanitize_in(ctx) {
                                return false;
                            }
                        }
                    }
                })
            }
            FieldType::Struct { typ } if with_parent.contains(typ) => checks.push(quote! {
                if !self.#fname().fast_sanitize_in(ctx) {
                    return false;
                }
            }),
            _ => {}
        }
    }

    let where_clause = generic.map(|t| quote!( where #t: Table<'a, Args = ()> + FastSanitize<'a> ));
    let ctx_param = if uses_ctx { quote!(ctx) } else { quote!(_ctx) };
    let body = if is_table {
        quote! {
            if !#ctx_param.enter(#data, <Self as Table<'a>>::MIN_SIZE) {
                return false;
            }
            #( #checks )*
            #ctx_param.exit();
            true
        }
    } else {
        quote! {
            #( #checks )*
            true
        }
    };
    quote! {
        #[cfg(feature = "fast_sanitize")]
        impl<'a, #generic> FastSanitize<'a> for #name<'a, #generic> #where_clause {
            fn fast_sanitize_in(&self, #ctx_param: &mut FastSanitizeContext) -> bool {
                #body
            }
        }
    }
}
