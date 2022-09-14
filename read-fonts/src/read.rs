//! Traits for interpreting font data

use font_types::Tag;

use crate::font_data::FontData;

/// A type that can be parsed from raw table data.
pub trait FontRead<'a>: Sized {
    fn read(data: FontData<'a>) -> Result<Self, ReadError>;
}

//NOTE: this is separate so that it can be a super trait of FontReadWithArgs and
//ComputeSize, without them needing to know about each other? I'm not sure this
//is necessary, but I don't know the full heirarchy of traits I'm going to need
//yet, so this seems... okay?

/// A trait for a type that needs additional arguments to be read.
pub trait ReadArgs {
    type Args: Copy;
}

/// A trait for types that require external data in order to be constructed.
pub trait FontReadWithArgs<'a>: Sized + ReadArgs {
    //type Args;
    /// read an item, using the provided args.
    ///
    /// If successful, returns a new item of this type, and the number of bytes
    /// used to construct it.
    ///
    /// If a type requires multiple arguments, they will be passed as a tuple.
    //TODO: split up the 'takes args' and 'reports size' parts of this into
    // separate traits
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError>;
}

/// A trait for tables that have multiple possible formats.
pub trait Format<T> {
    /// The format value for this table.
    const FORMAT: T;
}

/// A type that can compute its size at runtime, based on some input.
pub trait ComputeSize: ReadArgs {
    /// Compute the number of bytes required to represent this type.
    fn compute_size(args: &Self::Args) -> usize;
}

/// An error that occurs when reading font data
#[derive(Debug, Clone)]
pub enum ReadError {
    OutOfBounds,
    // i64 is flexible enough to store any value we might encounter
    InvalidFormat(i64),
    InvalidSfnt(u32),
    InvalidArrayLen,
    ValidationError,
    NullOffset,
    TableIsMissing(Tag),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReadError::OutOfBounds => write!(f, "An offset was out of bounds"),
            ReadError::InvalidFormat(x) => write!(f, "Invalid format '{x}'"),
            ReadError::InvalidSfnt(ver) => write!(f, "Invalid sfnt version 0x{ver:08X}"),
            ReadError::InvalidArrayLen => {
                write!(f, "Specified array length not a multiple of item size")
            }
            ReadError::ValidationError => write!(f, "A validation error occured"),
            ReadError::NullOffset => write!(f, "An offset was unexpectedly null"),
            ReadError::TableIsMissing(tag) => write!(f, "the {tag} table is missing"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ReadError {}
