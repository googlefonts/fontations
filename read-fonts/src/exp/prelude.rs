//! Everything the generated code in `read-fonts/generated/exp` refers to.

pub use bytemuck;
pub use core::ops::Range;
pub use font_types::*;

#[cfg(feature = "fast_sanitize")]
pub use super::parse::fast_sanitize::{Context as FastSanitizeContext, FastSanitize};
#[cfg(feature = "sanitize")]
pub use super::parse::sanitize::{Context as SanitizeContext, Problem, Sanitize};
pub use super::parse::{
    Array, ArrayElement, Bytes, ComputedSize, Discriminant, OffsetTo, Resolve, StridedStore, Table,
    VariableSize, VariableSizeArray, WithParent,
};
pub use crate::TopLevelTable;

/// A table with a `format` field, as the generated match arms read it.
pub trait Format<T> {
    const FORMAT: T;
}

/// The named transforms `#[count(..)]` expressions use.
pub mod transforms {
    pub use crate::codegen_prelude::transforms::*;
}
