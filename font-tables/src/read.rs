//! Traits for interpreting font data

use crate::font_data::FontData;

/// A type that can be parsed from raw table data.
pub trait FontRead<'a>: Sized {
    fn read(data: FontData<'a>) -> Result<Self, ReadError>;
}

/// A trait for types that require external data in order to be constructed.
pub trait FontReadWithArgs<'a, Args>: Sized {
    /// read an item, using the provided args.
    ///
    /// If successful, returns a new item of this type, and the number of bytes
    /// used to construct it.
    ///
    /// If a type requires multiple arguments, they will be passed as a tuple.
    //TODO: split up the 'takes args' and 'reports size' parts of this into
    // separate traits
    fn read_with_args(data: FontData<'a>, args: &Args) -> Result<Self, ReadError>;
}

/// A trait for tables that have multiple possible formats.
pub trait Format<T> {
    /// The format value for this table.
    const FORMAT: T;
}

/// A type that can compute its size at runtime, based on some input.
pub trait ComputeSize<Args> {
    /// Compute the number of bytes required to represent this type.
    fn compute_size(args: &Args) -> usize;
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
