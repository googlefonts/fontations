//! 16-bit signed and unsigned font-units

use crate::integers::{RawI16, RawU16};

/// 16-bit signed quantity in font design units.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FWord(i16);

/// 16-bit unsigned quantity in font design units.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UfWord(u16);

/// A raw (big-endian) [`FWord`].
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawFWord(RawI16);

/// A raw (big-endian) [`UfWord`].
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawUfWord(RawU16);

impl FWord {
    pub fn new(raw: i16) -> Self {
        Self(raw)
    }
}

impl UfWord {
    pub fn new(raw: u16) -> Self {
        Self(raw)
    }
}

crate::newtype_raw_type!(RawFWord, FWord);
crate::newtype_raw_type!(RawUfWord, UfWord);
