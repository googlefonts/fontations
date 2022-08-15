//! Raw types for compiling opentype tables

#[cfg(feature = "parsing")]
mod from_obj;
mod graph;
#[cfg(test)]
mod hex_diff;
pub mod layout;
mod offsets;
pub mod tables;
mod validate;
mod write;

pub use write::dump_table;

pub mod compile_prelude {
    use std::num::TryFromIntError;

    pub use super::offsets::{NullableOffsetMarker, OffsetMarker, WIDTH_16, WIDTH_24, WIDTH_32};
    pub use super::validate::{Validate, ValidationCtx};
    pub use super::write::{FontWrite, TableWriter};
    pub use font_types::*;

    #[cfg(feature = "parsing")]
    pub use super::from_obj::{FromObjRef, FromTableRef, ToOwnedObj, ToOwnedTable};
    #[cfg(feature = "parsing")]
    pub use font_tables::parse_prelude::{
        FontData, FontRead, FontReadWithArgs, ReadArgs, ReadError, ResolveOffset,
    };

    /// checked conversion to u16
    pub fn array_len<T>(s: &[T]) -> Result<u16, TryFromIntError> {
        s.len().try_into()
    }

    pub fn plus_one(val: &usize) -> Result<u16, TryFromIntError> {
        val.saturating_add(1).try_into()
    }
}
