//! Typed font tables

use super::read::{FontRead, Format, ReadError};
use crate::{
    font_data::FontData,
    offset::{Offset, ResolveOffset},
};
use std::ops::Range;
/// Return the minimum range of the table bytes
///
/// This trait is implemented in generated code, and we use this to get the minimum length/bytes of a table  
pub trait MinByteRange {
    fn min_byte_range(&self) -> Range<usize>;
}

#[derive(Clone)]
/// Typed access to raw table data.
pub struct TableRef<'a, T, F> {
    pub(crate) shape: T,
    pub(crate) fixed_fields: &'a F,
    pub(crate) data: FontData<'a>,
}

impl<'a, T, F> TableRef<'a, T, F> {
    /// Resolve the provided offset from the start of this table.
    pub fn resolve_offset<O: Offset, R: FontRead<'a>>(&self, offset: O) -> Result<R, ReadError> {
        offset.resolve(self.data)
    }

    /// Return a reference to this table's raw data.
    ///
    /// We use this in the compile crate to resolve offsets.
    pub fn offset_data(&self) -> FontData<'a> {
        self.data
    }

    /// Return a reference to the table's 'Shape' struct.
    ///
    /// This is a low level implementation detail, but it can be useful in
    /// some cases where you want to know things about a table's layout, such
    /// as the byte offsets of specific fields.
    pub fn shape(&self) -> &T {
        &self.shape
    }

    /// Returns a reference to a structure containing all of the fixed size
    /// fields at the start of the table.
    pub fn fixed_fields(&self) -> &'a F {
        self.fixed_fields
    }
}

// a blanket impl so that the format is available through a TableRef
impl<U, T: Format<U>, F> Format<U> for TableRef<'_, T, F> {
    const FORMAT: U = T::FORMAT;
}

impl<'a, T: MinByteRange, F> TableRef<'a, T, F> {
    /// Return the minimum byte range of this table
    pub fn min_byte_range(&self) -> Range<usize> {
        self.shape.min_byte_range()
    }

    /// Return the minimum bytes of this table
    pub fn min_table_bytes(&self) -> &'a [u8] {
        self.offset_data()
            .as_bytes()
            .get(self.shape.min_byte_range())
            .unwrap_or_default()
    }
}
