use std::ops::Range;

use font_types::ReadScalar;

pub trait TableInfo: Sized {
    type Info: Copy;
    fn parse<'a>(data: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError>;
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
pub struct Cursor<'a> {
    pos: usize,
    data: FontData<'a>,
}

#[derive(Debug, Clone)]
pub enum ReadError {
    OutOfBounds,
    InvalidFormat(u16),
    InvalidArrayLen,
    ValidationError,
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

    pub unsafe fn read_at_unchecked<T: ReadScalar>(&self, offset: usize) -> T {
        T::read(self.bytes.get_unchecked(offset..)).unwrap_unchecked()
    }

    fn check_in_bounds(&self, offset: usize) -> Result<(), ReadError> {
        self.bytes
            .get(offset)
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

    pub(crate) fn cursor(&self) -> Cursor<'a> {
        Cursor {
            pos: 0,
            data: self.clone(),
        }
    }
}

//fn aligned_to(bytes: &[u8], align: usize) -> bool {
//(bytes as *const _ as *const () as usize) % align == 0
//}

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
    pub(crate) fn read_validate<T, F>(&mut self, f: F) -> Result<T, ReadError>
    where
        T: ReadScalar,
        F: FnOnce(&T) -> bool,
    {
        let temp = self.read()?;
        if f(&temp) {
            Ok(temp)
        } else {
            Err(ReadError::ValidationError)
        }
    }

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

    pub(crate) fn finish<T: TableInfo>(self, shape: T::Info) -> Result<TableRef<'a, T>, ReadError> {
        let data = self.data;
        data.check_in_bounds(self.pos)?;
        Ok(TableRef { data, shape })
    }
}
