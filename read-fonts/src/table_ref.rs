//! Typed font tables

use crate::{
    font_data::FontData,
    offset::{Offset, ResolveOffset},
    FontReadWithArgs, Format, ReadError,
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
pub struct TableRef<'a, T> {
    pub(crate) shape: T,
    pub(crate) data: FontData<'a>,
}

impl<'a, T> TableRef<'a, T> {
    /// Resolve the provided offset from the start of this table.
    pub fn resolve_offset<O: Offset, R: FontReadWithArgs<'a, Args = ()>>(
        &self,
        offset: O,
    ) -> Result<R, ReadError> {
        offset.resolve_with_args(self.data, &())
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
    #[deprecated(note = "just use the base type directly")]
    pub fn shape(&self) -> &Self {
        &self
    }
}

// a blanket impl so that the format is available through a TableRef
impl<U, T: Format<U>> Format<U> for TableRef<'_, T> {
    const FORMAT: U = T::FORMAT;
}

impl<'a, T> TableRef<'a, T>
where
    Self: MinByteRange,
{
    /// Return the minimum bytes of this table
    pub fn min_table_bytes(&self) -> &'a [u8] {
        self.offset_data()
            .as_bytes()
            .get(self.min_byte_range())
            .unwrap_or_default()
    }
}
