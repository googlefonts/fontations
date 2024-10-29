//! Typed font tables

use types::Tag;

use super::read::{FontRead, Format, ReadError};
use crate::{
    font_data::FontData,
    offset::{Offset, ResolveOffset},
};

#[derive(Clone)]
/// Typed access to raw table data.
pub struct TableRef<'a, T> {
    pub(crate) shape: T,
    pub(crate) data: FontData<'a>,
}

impl<'a, T> TableRef<'a, T> {
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
impl<U, T: Format<U>> Format<U> for TableRef<'_, T> {
    const FORMAT: U = T::FORMAT;
}

/// Combination of a tag and a child table.
///
/// This is used in cases where a data structure has an array of records
/// where each record contains a tag and an offset to a table. This allows
/// us to provide convenience methods that return both values.
#[derive(Clone)]
pub struct TaggedElement<T> {
    pub tag: Tag,
    pub element: T,
}

impl<T> TaggedElement<T> {
    pub fn new(tag: Tag, element: T) -> Self {
        Self { tag, element }
    }
}

impl<T> std::ops::Deref for TaggedElement<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.element
    }
}
