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

use std::io::Write;

pub use fixed::{F2Dot14, Fixed};
pub use fword::{FWord, UfWord};
pub use longdatetime::LongDateTime;
pub use offset::{Offset, Offset16, Offset24, Offset32, OffsetHost, OffsetLen};
pub use raw::{BigEndian, Scalar};
pub use tag::Tag;
pub use uint24::Uint24;
pub use var_array::VarArray;
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

/// a type that can write itself into a buffer.
///
/// This is really just for scalars? As soon as we need to handle anything
/// containing offsets we have to get fancy.
//TODO: it would be nice if `Scalar` was a more general kind of 'be-convertable' trait?
//and then it could also do this job? This probably requires complex generic constants
// https://github.com/rust-lang/rust/issues/76560
pub trait FontWrite {
    fn write<W: Write>(&self, writer: &mut W);
}

//HACK: I'm not sure how this should work
/// A trait for types with variable length.
///
/// Currently we implement this by hand where necessary; it is only necessary
/// if these types occur in an array?
#[allow(clippy::len_without_is_empty)]
pub trait VarSized<'a>: FontRead<'a> {
    fn len(&self) -> usize;
}

impl<'a, T: zerocopy::FromBytes + zerocopy::Unaligned> FontRead<'a> for T {
    fn read(bytes: &'a [u8]) -> Option<Self> {
        T::read_from_prefix(bytes)
    }
}

macro_rules! write_be_bytes {
    ($ty:ty) => {
        impl crate::FontWrite for $ty {
            #[inline]
            fn write<W: Write>(&self, writer: &mut W) {
                writer.write_all(&self.to_be_bytes()).unwrap();
            }
        }
    };
}

write_be_bytes!(u8);
write_be_bytes!(i8);
write_be_bytes!(u16);
write_be_bytes!(i16);
write_be_bytes!(u32);
write_be_bytes!(i32);
write_be_bytes!(i64);
write_be_bytes!(Uint24);
write_be_bytes!(F2Dot14);
write_be_bytes!(Fixed);
write_be_bytes!(LongDateTime);
write_be_bytes!(Tag);
write_be_bytes!(Version16Dot16);

impl<const N: usize> FontWrite for [u8; N] {
    #[inline]
    fn write<W: Write>(&self, writer: &mut W) {
        writer.write_all(self.as_slice()).unwrap();
    }
}

impl<T: FontWrite> FontWrite for &'_ [T] {
    #[inline]
    fn write<W: Write>(&self, writer: &mut W) {
        self.iter().for_each(|x| x.write(writer))
    }
}
