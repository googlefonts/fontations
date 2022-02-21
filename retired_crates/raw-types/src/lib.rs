extern crate self as raw_types;

use zerocopy::{FromBytes, Unaligned, BE, I16, I32, I64, U16, U32};

pub mod layout;
pub mod tables;
mod var_array;

pub type Int8 = i8;
pub type Uint8 = u8;
pub type Int16 = I16<BE>;
pub type Uint16 = U16<BE>;
pub type Uint24 = [u8; 3];
pub type Int32 = I32<BE>;
pub type Uint32 = U32<BE>;

pub type Fixed = Int32;
pub type FWord = Int16;
pub type UfWord = Int16;
pub type F2Dot14 = Int16;
pub type LongDateTime = I64<BE>;

pub type Offset16 = Uint16;
pub type Offset24 = Uint24;
pub type Offset32 = Uint32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Unaligned, FromBytes)]
#[repr(C)]
pub struct Tag([u8; 4]);
pub type Version16Dot16 = Uint32;

pub use var_array::VarArray;

/// A convenience type for version stored as separate major/minor values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Unaligned, FromBytes)]
#[repr(C)]
pub struct MajorMinor(Uint16, Uint16);

impl MajorMinor {
    /// Construct a new version.
    pub const fn new(major: u16, minor: u16) -> Self {
        MajorMinor(
            Uint16::from_bytes(major.to_be_bytes()),
            Uint16::from_bytes(minor.to_be_bytes()),
        )
    }
}

/// A type that can be read from some chunk of bytes.
pub trait FontRead<'a>: Sized {
    /// attempt to read self from raw bytes.
    ///
    /// `bytes` may contain 'extra' bytes; the implemention should ignore them.
    fn read(bytes: &'a [u8]) -> Option<Self>;
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

impl<'a, T: FromBytes> FontRead<'a> for T {
    fn read(bytes: &'a [u8]) -> Option<Self> {
        T::read_from_prefix(bytes)
    }
}
