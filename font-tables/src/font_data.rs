//! raw font bytes

use std::ops::{Bound, Range, RangeBounds};

use font_types::ReadScalar;

use crate::read::{FontReadWithArgs, ReadError};
use crate::table_ref::TableRef;

#[derive(Debug, Default, Clone, Copy)]
pub struct FontData<'a> {
    total_pos: u32,
    bytes: &'a [u8],
}

/// A cursor for validating bytes during parsing.
///
/// This type improves the ergonomics of validation blah blah
///
/// # Note
///
/// call `finish` when you're done to ensure you're in bounds
pub struct Cursor<'a> {
    pos: usize,
    data: FontData<'a>,
}

impl<'a> FontData<'a> {
    /// Create a new `FontData` with these bytes.
    ///
    /// You generally don't need to do this? It is handled for you when loading
    /// data from disk, but may be useful in tests.
    pub fn new(bytes: &'a [u8]) -> Self {
        FontData {
            total_pos: 0,
            bytes,
        }
    }

    pub fn split_off(&self, pos: usize) -> Option<FontData<'a>> {
        self.bytes.get(pos..).map(|bytes| FontData {
            bytes,
            total_pos: self.total_pos.saturating_add(pos as u32),
        })
    }

    pub fn slice(&self, range: impl RangeBounds<usize>) -> Option<FontData<'a>> {
        let start = match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(i) => *i,
            Bound::Excluded(i) => i.saturating_add(1),
        };

        let bounds = (range.start_bound().cloned(), range.end_bound().cloned());
        let total_pos = self.total_pos.saturating_add(start as u32);
        self.bytes
            .get(bounds)
            .map(|bytes| FontData { bytes, total_pos })
    }

    pub fn read_at<T: ReadScalar>(&self, offset: usize) -> Result<T, ReadError> {
        self.bytes
            .get(offset..)
            .and_then(T::read)
            .ok_or_else(|| ReadError::OutOfBounds)
    }

    pub fn read_at_with_args<T>(&self, offset: usize, args: &T::Args) -> Result<T, ReadError>
    where
        T: FontReadWithArgs<'a>,
    {
        self.split_off(offset)
            .ok_or(ReadError::OutOfBounds)
            .and_then(|data| T::read_with_args(data, args))
    }

    pub unsafe fn read_at_unchecked<T: ReadScalar>(&self, offset: usize) -> T {
        T::read(self.bytes.get_unchecked(offset..)).unwrap_unchecked()
    }

    fn check_in_bounds(&self, offset: usize) -> Result<(), ReadError> {
        self.bytes
            .get(..offset)
            .ok_or_else(|| ReadError::OutOfBounds)
            .map(|_| ())
    }

    pub fn read_array<T>(&self, range: Range<usize>) -> Result<&'a [T], ReadError> {
        assert_ne!(std::mem::size_of::<T>(), 0);
        assert_eq!(std::mem::align_of::<T>(), 1);
        let bytes = self
            .bytes
            .get(range.clone())
            .ok_or_else(|| ReadError::OutOfBounds)?;
        if bytes.len() % std::mem::size_of::<T>() != 0 {
            return Err(ReadError::InvalidArrayLen);
        };
        unsafe { Ok(self.read_array_unchecked(range)) }
    }

    pub unsafe fn read_array_unchecked<T>(&self, range: Range<usize>) -> &'a [T] {
        let bytes = self.bytes.get_unchecked(range);
        let elems = bytes.len() / std::mem::size_of::<T>();
        std::slice::from_raw_parts(bytes.as_ptr() as *const _, elems)
    }

    //pub fn resolve_offset<T: FontRead<'a>, O: Offset>(&self, off: O) -> Result<T, ReadError> {
    //let off = off.non_null().ok_or(ReadError::NullOffset)?;
    //self.split_off(off)
    //.ok_or(ReadError::OutOfBounds)
    //.and_then(|data| T::read(data))
    //}

    pub(crate) fn cursor(&self) -> Cursor<'a> {
        Cursor {
            pos: 0,
            data: self.clone(),
        }
    }
}

impl<'a> Cursor<'a> {
    pub(crate) fn advance<T: ReadScalar>(&mut self) {
        self.pos += T::RAW_BYTE_LEN
    }

    pub(crate) fn advance_by(&mut self, n_bytes: usize) {
        self.pos += n_bytes;
    }

    pub(crate) fn read<T: ReadScalar>(&mut self) -> Result<T, ReadError> {
        let temp = self.data.read_at(self.pos);
        self.pos += T::RAW_BYTE_LEN;
        temp
    }

    /// read a value, validating it with the provided function if successful.
    //pub(crate) fn read_validate<T, F>(&mut self, f: F) -> Result<T, ReadError>
    //where
    //T: ReadScalar,
    //F: FnOnce(&T) -> bool,
    //{
    //let temp = self.read()?;
    //if f(&temp) {
    //Ok(temp)
    //} else {
    //Err(ReadError::ValidationError)
    //}
    //}

    //pub(crate) fn check_array<T: Scalar>(&mut self, len_bytes: usize) -> Result<(), ReadError> {
    //assert_ne!(std::mem::size_of::<BigEndian<T>>(), 0);
    //assert_eq!(std::mem::align_of::<BigEndian<T>>(), 1);
    //if len_bytes % T::SIZE != 0 {
    //return Err(ReadError::InvalidArrayLen);
    //}
    //self.data.check_in_bounds(self.pos + len_bytes)
    //todo!()
    //}

    /// return the current position, or an error if we are out of bounds
    pub(crate) fn position(&self) -> Result<usize, ReadError> {
        self.data.check_in_bounds(self.pos).map(|_| self.pos)
    }

    pub(crate) fn finish<T>(self, shape: T) -> Result<TableRef<'a, T>, ReadError> {
        let data = self.data;
        data.check_in_bounds(self.pos)?;
        Ok(TableRef { data, shape })
    }
}
