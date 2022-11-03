//! The pre-compilation validation pass

use std::{
    collections::BTreeSet,
    fmt::{Debug, Display},
    ops::Deref,
};

use crate::offsets::{NullableOffsetMarker, OffsetMarker};

/// Pre-compilation validation of tables.
///
/// The OpenType specification describes various requirements for different
/// tables that are awkward to encode in the type system, such as requiring
/// certain arrays to have equal lengths. These requirements are enforced
/// via a validation pass.
pub trait Validate {
    /// Ensure that this table is well-formed, reporting any errors.
    ///
    /// This is an auto-generated method that calls to [validate_impl][Self::validate_impl] and
    /// collects any errors.
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut ctx = Default::default();
        self.validate_impl(&mut ctx);
        if ctx.errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationReport { errors: ctx.errors })
        }
    }

    /// Validate this table.
    ///
    /// If you need to implement this directly, it should look something like:
    ///
    /// ```rust
    /// # use write_fonts::validate::{Validate, ValidationCtx};
    /// struct MyRecord {
    ///     my_values: Vec<u16>,
    /// }
    ///
    /// impl Validate for MyRecord {
    ///     fn validate_impl(&self, ctx: &mut ValidationCtx) {
    ///         ctx.in_table("MyRecord", |ctx| {
    ///             ctx.in_field("my_values", |ctx| {
    ///                 if self.my_values.len() > (u16::MAX as usize) {
    ///                     ctx.report("array is too long");
    ///                 }
    ///             })
    ///         })
    ///     }
    /// }
    /// ```
    #[allow(unused_variables)]
    fn validate_impl(&self, ctx: &mut ValidationCtx);
}

/// A context for collecting validation error.
///
/// This is responsible for tracking the position in the tree at which
/// a given error is reported.
///
/// ## paths/locations
///
/// As validation travels down through the object graph, the path is recorded
/// via appropriate calls to methods like [in_table][Self::in_table] and [in_field][Self::in_field].
#[derive(Clone, Debug, Default)]
pub struct ValidationCtx {
    cur_location: Vec<LocationElem>,
    errors: Vec<ValidationError>,
}

#[derive(Debug, Clone)]
struct ValidationError {
    error: String,
    location: Vec<LocationElem>,
}

/// One or more validation errors.
pub struct ValidationReport {
    errors: Vec<ValidationError>,
}

#[derive(Debug, Clone)]
enum LocationElem {
    Table(&'static str),
    Field(&'static str),
    Index(usize),
}

impl ValidationCtx {
    /// Run the provided closer in the context of a new table.
    ///
    /// Errors reported in the closure will include the provided identifer
    /// in their path.
    pub fn in_table(&mut self, name: &'static str, f: impl FnOnce(&mut ValidationCtx)) {
        self.with_elem(LocationElem::Table(name), f);
    }

    /// Run the provided closer in the context of a new field.
    ///
    /// Errors reported in the closure will be associated with the field.
    pub fn in_field(&mut self, name: &'static str, f: impl FnOnce(&mut ValidationCtx)) {
        self.with_elem(LocationElem::Field(name), f);
    }

    /// Run the provided closer in the context of an array.
    pub fn in_array(&mut self, f: impl FnOnce(&mut ValidationCtx)) {
        self.with_elem(LocationElem::Index(0), f);
    }

    /// Run the provided closer in the context of a new array item.
    ///
    /// This must only be called in a closure passed to [in_array][Self::in_array].
    pub fn array_item(&mut self, f: impl FnOnce(&mut ValidationCtx)) {
        assert!(matches!(
            self.cur_location.last(),
            Some(LocationElem::Index(_))
        ));
        f(self);
        match self.cur_location.last_mut() {
            Some(LocationElem::Index(i)) => *i += 1,
            _ => panic!("array_item called outside of array"),
        }
    }

    /// Report a new error, associating it with the current path.
    pub fn report(&mut self, msg: impl Display) {
        self.errors.push(ValidationError {
            location: self.cur_location.clone(),
            error: msg.to_string(),
        });
    }

    fn with_elem(&mut self, elem: LocationElem, f: impl FnOnce(&mut ValidationCtx)) {
        self.cur_location.push(elem);
        f(self);
        self.cur_location.pop();
    }
}

impl Display for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.errors.len() == 1 {
            return writeln!(f, "Validation error:\n{}", self.errors.first().unwrap());
        }

        writeln!(f, "{} validation errors:", self.errors.len())?;
        for (i, error) in self.errors.iter().enumerate() {
            writeln!(f, "#{}\n{error}", i + 1)?;
        }
        Ok(())
    }
}

impl Debug for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

static MANY_SPACES: &str = "                                                                                                        ";

impl Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\"{}\"", self.error)?;
        let mut indent = 0;

        for (i, window) in self.location.windows(2).enumerate() {
            let prev = &window[0];
            let current = &window[1];
            if i == 0 {
                if let LocationElem::Table(name) = prev {
                    write!(f, "in: {name}")?;
                } else {
                    panic!("first item always table");
                }
            }

            match current {
                LocationElem::Table(name) => {
                    indent += 1;
                    let indent_str = &MANY_SPACES[..indent * 2];
                    write!(f, "\n{indent_str}{name}")
                }
                LocationElem::Field(name) => write!(f, ".{name}"),
                LocationElem::Index(idx) => write!(f, "[{idx}]"),
            }?;
        }
        writeln!(f)
    }
}

impl<T: Validate> Validate for Vec<T> {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        ctx.in_array(|ctx| {
            for item in self.iter() {
                ctx.array_item(|ctx| {
                    item.validate_impl(ctx);
                })
            }
        });
    }
}

impl<const N: usize, T: Validate> Validate for OffsetMarker<T, N> {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        self.deref().validate_impl(ctx)
    }
}

impl<const N: usize, T: Validate> Validate for NullableOffsetMarker<T, N> {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        self.deref().validate_impl(ctx)
    }
}

impl<T: Validate> Validate for Option<T> {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        match self {
            Some(t) => t.validate_impl(ctx),
            None => (),
        }
    }
}

impl<T: Validate> Validate for BTreeSet<T> {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        ctx.in_array(|ctx| {
            for item in self.iter() {
                ctx.array_item(|ctx| {
                    item.validate_impl(ctx);
                })
            }
        });
    }
}
