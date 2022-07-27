//! Traits for interpreting font data

use crate::font_data::FontData;

/// A type that can be parsed from raw table data.
pub trait FontRead<'a>: Sized {
    fn read(data: FontData<'a>) -> Result<Self, ReadError>;
}

/// A trait for tables that have multiple possible formats.
pub trait Format<T> {
    /// The format value for this table.
    const FORMAT: T;
}

/// An error that occurs when reading font data
#[derive(Debug, Clone)]
pub enum ReadError {
    OutOfBounds,
    InvalidFormat(u16),
    InvalidArrayLen,
    ValidationError,
    NullOffset,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Some error I guess")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ReadError {}
