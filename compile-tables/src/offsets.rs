//! compile-time representations of offsets

use font_types::Offset;

#[cfg(feature = "parsing")]
use super::compile_prelude::{FromTableRef, ReadError};

use super::write::{FontWrite, TableWriter};

/// An offset subtable.
#[derive(Clone)]
pub struct OffsetMarker<W, T> {
    width: std::marker::PhantomData<W>,
    obj: Option<T>,
}

/// An offset subtable which may be null.
#[derive(Clone)]
pub struct NullableOffsetMarker<W, T> {
    width: std::marker::PhantomData<W>,
    obj: Option<T>,
}

impl<W, T> OffsetMarker<W, T> {
    //TODO: how do we handle malformed inputs? do we error earlier than this?
    /// Get the object. Fonts in the wild may be malformed, so this still returns
    /// an option?
    pub fn get(&self) -> Option<&T> {
        self.obj.as_ref()
    }
}

impl<W: Offset, T> OffsetMarker<W, T> {
    /// Create a new marker.
    pub fn new(obj: T) -> Self {
        OffsetMarker {
            width: std::marker::PhantomData,
            obj: Some(obj),
        }
    }

    /// Creates a new marker with an object that may be null.
    //TODO: figure out how we're actually handling null offsets. Some offsets
    //are allowed to be null, but even offsets that aren't *may* be null,
    //and we should handle this.
    pub fn new_maybe_null(obj: Option<T>) -> Self {
        OffsetMarker {
            width: std::marker::PhantomData,
            obj,
        }
    }
}

impl<W, T> NullableOffsetMarker<W, T> {
    //TODO: how do we handle malformed inputs? do we error earlier than this?
    /// Get the object, if it exists.
    pub fn get(&self) -> Option<&T> {
        self.obj.as_ref()
    }
}

impl<W: Offset, T> NullableOffsetMarker<W, T> {
    pub fn new(obj: Option<T>) -> Self {
        NullableOffsetMarker {
            width: std::marker::PhantomData,
            obj,
        }
    }
}

impl<W: Offset, T: FontWrite> FontWrite for OffsetMarker<W, T> {
    fn write_into(&self, writer: &mut TableWriter) {
        match self.obj.as_ref() {
            Some(obj) => writer.write_offset::<W>(obj),
            None => {
                eprintln!("warning: unexpected null OffsetMarker");
                writer.write_slice(W::null_bytes());
            }
        }
    }
}

impl<W: Offset, T: FontWrite> FontWrite for NullableOffsetMarker<W, T> {
    fn write_into(&self, writer: &mut TableWriter) {
        match self.obj.as_ref() {
            Some(obj) => writer.write_offset::<W>(obj),
            None => writer.write_slice(W::null_bytes()),
        }
    }
}

#[cfg(feature = "parsing")]
impl<W, T, U> From<Result<U, ReadError>> for OffsetMarker<W, T>
where
    W: Offset,
    T: FromTableRef<U>,
{
    fn from(from: Result<U, ReadError>) -> Self {
        OffsetMarker::new_maybe_null(from.ok().map(|x| T::from_table_ref(&x)))
    }
}

#[cfg(feature = "parsing")]
impl<W, T, U> From<Option<Result<U, ReadError>>> for NullableOffsetMarker<W, T>
where
    W: Offset,
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

impl<W, T> Default for OffsetMarker<W, T> {
    fn default() -> Self {
        OffsetMarker {
            width: std::marker::PhantomData,
            obj: None,
        }
    }
}

impl<W, T> Default for NullableOffsetMarker<W, T> {
    fn default() -> Self {
        NullableOffsetMarker {
            width: std::marker::PhantomData,
            obj: None,
        }
    }
}

impl<W, T: PartialEq> PartialEq for OffsetMarker<W, T> {
    fn eq(&self, other: &Self) -> bool {
        self.obj == other.obj
    }
}

impl<W, T: PartialEq> PartialEq for NullableOffsetMarker<W, T> {
    fn eq(&self, other: &Self) -> bool {
        self.obj == other.obj
    }
}

impl<W: Offset, T: std::fmt::Debug> std::fmt::Debug for OffsetMarker<W, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "OffsetMarker({}, {:?})",
            W::RAW_BYTE_LEN * 8,
            self.obj.as_ref(),
        )
    }
}

impl<W: Offset, T: std::fmt::Debug> std::fmt::Debug for NullableOffsetMarker<W, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "NullableOffsetMarker({}, {:?})",
            W::RAW_BYTE_LEN * 8,
            self.obj.as_ref(),
        )
    }
}
