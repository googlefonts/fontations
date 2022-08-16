//! compile-time representations of offsets

#[cfg(feature = "parsing")]
use crate::from_obj::FromTableRef;
#[cfg(feature = "parsing")]
use read_fonts::ReadError;

use super::write::{FontWrite, TableWriter};

/// The width in bytes of an Offset16
pub const WIDTH_16: usize = 2;
/// The width in bytes of an Offset24
#[allow(dead_code)] // will be used one day :')
pub const WIDTH_24: usize = 3;
/// The width in bytes of an Offset32
pub const WIDTH_32: usize = 4;

/// An offset subtable.
///
/// The generic const `N` is the width of the offset, in bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OffsetMarker<T, const N: usize = WIDTH_16> {
    obj: Option<T>,
}

/// An offset subtable which may be null.
///
/// The generic const `N` is the width of the offset, in bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NullableOffsetMarker<T, const N: usize = WIDTH_16> {
    obj: Option<T>,
}

impl<const N: usize, T> OffsetMarker<T, N> {
    /// `true` if the offset is non-null
    pub fn is_some(&self) -> bool {
        self.obj.is_some()
    }

    /// `true` if the offset is null
    pub fn is_none(&self) -> bool {
        self.obj.is_none()
    }

    //TODO: how do we handle malformed inputs? do we error earlier than this?
    /// Get the object. Fonts in the wild may be malformed, so this still returns
    /// an option?
    pub fn get(&self) -> Option<&T> {
        self.obj.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.obj.as_mut()
    }

    pub fn set(&mut self, obj: T) {
        self.obj = Some(obj);
    }

    pub fn clear(&mut self) {
        self.obj = None;
    }
}

impl<const N: usize, T> OffsetMarker<T, N> {
    /// Create a new marker.
    pub fn new(obj: T) -> Self {
        OffsetMarker { obj: Some(obj) }
    }

    /// Creates a new marker with an object that may be null.
    //TODO: figure out how we're actually handling null offsets. Some offsets
    //are allowed to be null, but even offsets that aren't *may* be null,
    //and we should handle this.
    pub fn new_maybe_null(obj: Option<T>) -> Self {
        OffsetMarker { obj }
    }
}

impl<const N: usize, T> NullableOffsetMarker<T, N> {
    /// `true` if the offset is non-null
    pub fn is_some(&self) -> bool {
        self.obj.is_some()
    }

    /// `true` if the offset is null
    pub fn is_none(&self) -> bool {
        self.obj.is_none()
    }

    //TODO: how do we handle malformed inputs? do we error earlier than this?
    /// Get the object, if it exists.
    pub fn get(&self) -> Option<&T> {
        self.obj.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.obj.as_mut()
    }

    pub fn set(&mut self, obj: T) {
        self.obj = Some(obj);
    }

    pub fn clear(&mut self) {
        self.obj = None;
    }
}

impl<const N: usize, T> NullableOffsetMarker<T, N> {
    pub fn new(obj: Option<T>) -> Self {
        NullableOffsetMarker { obj }
    }
}

impl<const N: usize, T: FontWrite> FontWrite for OffsetMarker<T, N> {
    fn write_into(&self, writer: &mut TableWriter) {
        match self.obj.as_ref() {
            Some(obj) => writer.write_offset(obj, N),
            None => {
                eprintln!("warning: unexpected null OffsetMarker");
                writer.write_slice([0u8; N].as_slice());
            }
        }
    }
}

impl<const N: usize, T: FontWrite> FontWrite for NullableOffsetMarker<T, N> {
    fn write_into(&self, writer: &mut TableWriter) {
        match self.obj.as_ref() {
            Some(obj) => writer.write_offset(obj, N),
            None => writer.write_slice([0u8; N].as_slice()),
        }
    }
}

#[cfg(feature = "parsing")]
impl<const N: usize, T, U> From<Result<U, ReadError>> for OffsetMarker<T, N>
where
    T: FromTableRef<U>,
{
    fn from(from: Result<U, ReadError>) -> Self {
        OffsetMarker::new_maybe_null(from.ok().map(|x| T::from_table_ref(&x)))
    }
}

#[cfg(feature = "parsing")]
impl<const N: usize, T, U> From<Option<Result<U, ReadError>>> for NullableOffsetMarker<T, N>
where
    T: FromTableRef<U>,
{
    fn from(from: Option<Result<U, ReadError>>) -> Self {
        NullableOffsetMarker::new(
            from.transpose()
                .ok()
                .flatten()
                .map(|x| T::from_table_ref(&x)),
        )
    }
}

// In case I still want to use these?

//impl<T: std::fmt::Debug, const N: usize> std::fmt::Debug for OffsetMarker<T, N> {
//fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//write!(f, "OffsetMarker({}, {:?})", N * 8, self.obj.as_ref(),)
//}
//}

//impl<const N: usize, T: std::fmt::Debug> std::fmt::Debug for NullableOffsetMarker<T, N> {
//fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//write!(
//f,
//"NullableOffsetMarker({}, {:?})",
//N * 8,
//self.obj.as_ref(),
//)
//}
//}
