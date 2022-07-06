//! Common [scalar data types][data types] used in font files
//!
//! [data types]: https://docs.microsoft.com/en-us/typography/opentype/spec/otff#data-types

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(any(feature = "std", test))]
#[macro_use]
extern crate std;

#[cfg(all(not(feature = "std"), not(test)))]
#[macro_use]
extern crate core as std;

mod fixed;
mod fword;
mod longdatetime;
mod offset;
mod raw;
mod tag;
mod uint24;
mod var_array;
mod version;

#[doc(hidden)]
pub mod test_helpers;

pub use fixed::{F2Dot14, Fixed};
pub use fword::{FWord, UfWord};
pub use longdatetime::LongDateTime;
pub use offset::{Offset, Offset16, Offset24, Offset32, OffsetHost, OffsetLen};
pub use raw::{BigEndian, ReadScalar, Scalar};
pub use tag::Tag;
pub use uint24::Uint24;
pub use var_array::DynSizedArray;
pub use version::{MajorMinor, Version16Dot16};

//TODO: make me a struct
pub type GlyphId = u16;

/// A type that can be read from some chunk of bytes.
pub trait FontRead<'a>: Sized {
    /// attempt to read self from raw bytes.
    ///
    /// `bytes` may contain 'extra' bytes; the implemention should ignore them.
    fn read(bytes: &'a [u8]) -> Option<Self>;
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
    fn read_with_args(bytes: &'a [u8], args: &Args) -> Option<(Self, &'a [u8])>;
}

impl<'a, T: zerocopy::FromBytes + zerocopy::Unaligned> FontRead<'a> for T {
    fn read(bytes: &'a [u8]) -> Option<Self> {
        T::read_from_prefix(bytes)
    }
}
