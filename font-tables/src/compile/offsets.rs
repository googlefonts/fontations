//! compile-time representations of offsets

use font_types::{Offset, Offset16, Offset24, Offset32};

use super::FontWrite;

/// An offset subtable.
pub struct OffsetMarker<W, T> {
    width: std::marker::PhantomData<W>,
    obj: Option<T>,
}

/// An offset subtable which may be null.
pub struct NullableOffsetMarker<W, T> {
    width: std::marker::PhantomData<W>,
    obj: Option<T>,
}

/// Marker to a 32-bit subtable offset.
pub type OffsetMarker32<T> = OffsetMarker<Offset32, T>;

/// Marker to a 16-bit subtable offset.
pub type OffsetMarker16<T> = OffsetMarker<Offset16, T>;

/// Marker to a 24-bit subtable offset.
pub type OffsetMarker24<T> = OffsetMarker<Offset24, T>;

/// Marker to a nullable 32-bit subtable offset.
pub type NullableOffsetMarker32<T> = NullableOffsetMarker<Offset32, T>;

/// Marker to a nullable 16-bit subtable offset.
pub type NullableOffsetMarker16<T> = NullableOffsetMarker<Offset16, T>;

/// Marker to a nullable 24-bit subtable offset.
pub type NullableOffsetMarker24<T> = NullableOffsetMarker<Offset24, T>;

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

//impl<W: Offset, T: FontWrite> ToOwnedImpl for Offset16 {
//type Owned = ;
//}

impl<W: Offset, T: FontWrite> FontWrite for OffsetMarker<W, T> {
    fn write_into(&self, writer: &mut super::TableWriter) {
        match self.obj.as_ref() {
            Some(obj) => writer.write_offset::<W>(obj),
            None => {
                eprintln!("warning: unexpected null OffsetMarker");
                writer.write_slice(W::SIZE.null_bytes());
            }
        }
    }
}

impl<W: Offset, T: FontWrite> FontWrite for NullableOffsetMarker<W, T> {
    fn write_into(&self, writer: &mut super::TableWriter) {
        match self.obj.as_ref() {
            Some(obj) => writer.write_offset::<W>(obj),
            None => writer.write_slice(W::SIZE.null_bytes()),
        }
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

impl<W, T: PartialEq> PartialEq for OffsetMarker<W, T> {
    fn eq(&self, other: &Self) -> bool {
        self.obj == other.obj
    }
}

impl<W: Offset, T: std::fmt::Debug> std::fmt::Debug for OffsetMarker<W, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "OffsetMarker({}, {:?})", W::SIZE, self.obj.as_ref(),)
    }
}
