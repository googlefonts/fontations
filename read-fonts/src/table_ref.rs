//! Typed font tables

use super::read::{FontRead, Format, ReadError};
use crate::{
    font_data::FontData,
    offset::{Offset, ResolveOffset},
};
use std::marker::PhantomData;
use std::ops::Range;
/// Return the minimum range of the table bytes
///
/// This trait is implemented in generated code, and we use this to get the minimum length/bytes of a table  
pub trait MinByteRange {
    fn min_byte_range(&self) -> Range<usize>;
}

#[derive(Clone)]
/// Typed access to raw table data.
pub struct TableRef<'a, T, A = ()> {
    pub(crate) args: A,
    pub(crate) data: FontData<'a>,
    pub(crate) _marker: PhantomData<T>,
}

impl<'a, T, A> TableRef<'a, T, A> {
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
}

// a blanket impl so that the format is available through a TableRef
impl<U, T: Format<U>, A> Format<U> for TableRef<'_, T, A> {
    const FORMAT: U = T::FORMAT;
}

impl<'a, T, A> TableRef<'a, T, A>
where
    TableRef<'a, T, A>: MinByteRange,
{
    /// Return the minimum byte range of this table
    pub fn min_byte_range(&self) -> Range<usize> {
        MinByteRange::min_byte_range(self)
    }

    /// Return the minimum bytes of this table
    pub fn min_table_bytes(&self) -> &'a [u8] {
        self.offset_data()
            .as_bytes()
            .get(self.min_byte_range())
            .unwrap_or_default()
    }
}
