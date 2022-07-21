//! Raw types for compiling opentype tables

pub mod layout;

pub mod compile_prelude {
    use std::num::TryFromIntError;

    pub use font_tables::compile::*;
    pub use font_tables::tables::gpos::ValueRecord;
    pub use font_types::*;

    /// checked conversion to u16
    pub fn array_len<T>(s: &[T]) -> Result<u16, TryFromIntError> {
        s.len().try_into()
    }

    pub fn plus_one(val: &usize) -> Result<u16, TryFromIntError> {
        val.saturating_add(1).try_into()
    }
}
