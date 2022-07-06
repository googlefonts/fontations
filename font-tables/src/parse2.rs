//use std::ops::{Bound, RangeBounds};
//use std::slice::SliceIndex;

use font_types::ReadScalar;

pub trait TableInfo: Sized {
    type Info: Copy;
    fn parse<'a>(ctx: &mut ParseContext<'a>) -> Result<TableRef<'a, Self>, ReadError>;
}

pub trait Format<T> {
    const FORMAT: T;
}

pub trait FontRead<'a>: Sized {
    fn read(data: &FontData<'a>) -> Result<Self, ReadError>;
}

pub struct TableRef<'a, T: TableInfo> {
    pub(crate) shape: T::Info,
    pub(crate) data: FontData<'a>,
}

/// The font data as well as information for reporting errors during parsing.
pub struct ParseContext<'a> {
    data: FontData<'a>,
}

#[derive(Debug, Clone, Copy)]
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
pub(crate) struct Cursor<'a> {
    pos: usize,
    data: FontData<'a>,
}

#[derive(Debug, Clone)]
pub enum ReadError {
    OutOfBounds,
    InvalidBits,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Some error I guess")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ReadError {}

impl<'a> FontData<'a> {
    pub fn split_off(&self, pos: usize) -> Option<FontData<'a>> {
        self.bytes.get(pos..).map(|bytes| FontData {
            bytes,
            total_pos: self.total_pos.saturating_add(pos as u32),
        })
    }
    //pub fn get(&self, range: impl RangeBounds<usize>) -> Option<FontData<'a>> {
    //let start = match range.start_bound() {
    //Bound::Unbounded => 0,
    //Bound::Included(i) => *i,
    //Bound::Excluded(i) => i.saturating_add(1),
    //};

    //let bounds = (range.start_bound().cloned(), range.end_bound().cloned());
    //let total_pos = self.total_pos.saturating_add(start as u32);
    //self.bytes
    //.get(bounds)
    //.map(|bytes| FontData { bytes, total_pos })
    //}
    //pub fn get<I>(&self, range: I) -> Option<FontData<'a>>
    //where
    //I: SliceIndex<[u8], Output = [u8]>,
    //{
    //self.bytes.get(range).map(|bytes| FontData { bytes })
    //}

    pub fn read_at<T: ReadScalar>(&self, offset: usize) -> Result<T, ReadError> {
        self.bytes
            .get(offset..)
            .and_then(T::read)
            .ok_or_else(|| ReadError::OutOfBounds)
    }

    fn check_in_bounds(&self, offset: usize) -> Result<(), ReadError> {
        self.bytes
            .get(offset)
            .ok_or_else(|| ReadError::OutOfBounds)
            .map(|_| ())
    }
}

impl<'a> ParseContext<'a> {
    pub(crate) fn cursor(&self) -> Cursor<'a> {
        Cursor {
            pos: 0,
            data: self.data,
        }
    }
}

impl<'a> Cursor<'a> {
    pub(crate) fn advance<T: ReadScalar>(&mut self) {
        self.pos += T::SIZE
    }

    pub(crate) fn advance_by(&mut self, n_bytes: usize) {
        self.pos += n_bytes;
    }

    pub(crate) fn read<T: ReadScalar>(&mut self) -> Result<T, ReadError> {
        let temp = self.data.read_at(self.pos);
        self.pos += T::SIZE;
        temp
    }

    pub(crate) fn finish<T: TableInfo>(self, shape: T::Info) -> Result<TableRef<'a, T>, ReadError> {
        let data = self.data;
        data.check_in_bounds(self.pos)?;
        Ok(TableRef { data, shape })
    }
}
