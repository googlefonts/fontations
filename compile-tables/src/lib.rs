//! Raw types for compiling opentype tables

mod graph;
pub mod layout;
mod offsets;
mod write;

pub use write::dump_table;

pub mod compile_prelude {
    use std::num::TryFromIntError;

    pub use super::offsets::{NullableOffsetMarker, OffsetMarker};
    pub use super::write::{FontWrite, TableWriter};
    pub use font_types::*;

    /// checked conversion to u16
    pub fn array_len<T>(s: &[T]) -> Result<u16, TryFromIntError> {
        s.len().try_into()
    }

    pub fn plus_one(val: &usize) -> Result<u16, TryFromIntError> {
        val.saturating_add(1).try_into()
    }
}
