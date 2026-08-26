//! The bytes a table is read out of.
//!
//! [`Bytes`] is `FontData` cut down to what the framework asks of it, with one
//! change that matters: every read returns [`Option`] rather than
//! `Result<_, ReadError>`.
//!
//! That change is the point. `FontData::read_at` returns a `Result` whose error
//! says only `OutOfBounds`, so every accessor the emitter wrote had to append
//! `.ok()` to throw it away again. The information was never used and never
//! reached anyone: what a caller wants when a read fails is
//! [`sanitize`][super::sanitize], which reports where and why. Reading is
//! left to say the one thing it knows.

#![deny(clippy::arithmetic_side_effects)]

use bytemuck::AnyBitPattern;
use core::ops::{Range, RangeBounds};
use types::{FixedSize, Scalar};

// `Bytes` is `FontData` cut down to what the framework asks of it, with one
// change that matters: every read returns `Option` rather than
// `Result<_, ReadError>`.
//
// That change is the point. `FontData::read_at` returns a `Result` whose error
// says only `OutOfBounds`, so every accessor the emitter wrote had to append
// `.ok()` to throw it away again. The information was never used and never
// reached anyone: what a caller wants when a read fails is
// [`sanitize`][super::sanitize], which reports where and why. Reading is left
// to say the one thing it knows.
//
// It lives here rather than in a module of its own because it is the other
// half of what parsing is: the traits say what a table is, this says what the
// bytes under it can be asked.

/// A range of bytes to read a table out of.
#[derive(Debug, Default, Clone, Copy)]
pub struct Bytes<'a> {
    bytes: &'a [u8],
}

impl<'a> Bytes<'a> {
    /// No bytes at all.
    pub const EMPTY: Bytes<'static> = Bytes { bytes: &[] };

    #[inline]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Bytes { bytes }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Everything from `pos` on, or `None` if `pos` is past the end.
    ///
    /// How an offset is followed: the target's bytes run from where the offset
    /// points to the end of what contained it.
    #[inline]
    pub fn split_off(&self, pos: usize) -> Option<Bytes<'a>> {
        self.bytes.get(pos..).map(Bytes::new)
    }

    /// Just the bytes in `range`, or `None` if it does not fit.
    #[inline]
    pub fn slice(&self, range: impl RangeBounds<usize>) -> Option<Bytes<'a>> {
        let bounds = (range.start_bound().cloned(), range.end_bound().cloned());
        self.bytes.get(bounds).map(Bytes::new)
    }

    /// Reads a scalar at `pos`.
    #[inline]
    pub fn read_at<T: Scalar>(&self, pos: usize) -> Option<T> {
        let end = pos.checked_add(T::RAW_BYTE_LEN)?;
        self.bytes.get(pos..end).and_then(T::read)
    }

    /// Borrows a fixed-size record at `pos`.
    ///
    /// # Panics
    ///
    /// If `T` is zero-sized, has an alignment other than one, or has internal
    /// padding — none of which a generated record can have.
    #[inline]
    pub fn read_ref_at<T: AnyBitPattern + FixedSize>(&self, pos: usize) -> Option<&'a T> {
        let end = pos.checked_add(T::RAW_BYTE_LEN)?;
        self.bytes.get(pos..end).map(bytemuck::from_bytes)
    }

    /// Borrows `range` as a slice of fixed-size records.
    ///
    /// `None` if the range does not fit, or is not a whole number of records.
    ///
    /// # Panics
    ///
    /// As [`read_ref_at`][Self::read_ref_at].
    #[inline]
    pub fn read_array<T: AnyBitPattern + FixedSize>(&self, range: Range<usize>) -> Option<&'a [T]> {
        let bytes = self.bytes.get(range)?;
        if bytes.len().checked_rem(core::mem::size_of::<T>())? != 0 {
            return None;
        }
        Some(bytemuck::cast_slice(bytes))
    }
}

impl<'a> From<&'a [u8]> for Bytes<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Bytes::new(bytes)
    }
}

impl<'a> From<crate::FontData<'a>> for Bytes<'a> {
    /// The bridge into the existing world, for as long as both exist.
    fn from(data: crate::FontData<'a>) -> Self {
        Bytes::new(data.as_bytes())
    }
}

impl<'a> From<Bytes<'a>> for crate::FontData<'a> {
    fn from(bytes: Bytes<'a>) -> Self {
        crate::FontData::new(bytes.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_that_do_not_fit_are_none() {
        let bytes = Bytes::new(&[0, 1, 0, 2]);
        assert_eq!(bytes.read_at::<u16>(0), Some(1));
        assert_eq!(bytes.read_at::<u16>(2), Some(2));
        assert_eq!(bytes.read_at::<u16>(3), None);
        assert_eq!(bytes.read_at::<u32>(1), None);
        // a position past the end, and one that would overflow computing it
        assert_eq!(bytes.read_at::<u16>(99), None);
        assert_eq!(bytes.read_at::<u16>(usize::MAX), None);
    }

    #[test]
    fn an_array_must_be_a_whole_number_of_records() {
        let bytes = Bytes::new(&[0, 1, 0, 2, 0]);
        assert_eq!(bytes.read_array::<u8>(0..5).map(<[u8]>::len), Some(5));
        assert!(bytes
            .read_array::<font_types::BigEndian<u16>>(0..4)
            .is_some());
        // not a multiple of the record size
        assert!(bytes
            .read_array::<font_types::BigEndian<u16>>(0..5)
            .is_none());
        // past the end
        assert!(bytes
            .read_array::<font_types::BigEndian<u16>>(0..8)
            .is_none());
    }

    #[test]
    fn splitting_and_slicing() {
        let bytes = Bytes::new(&[1, 2, 3, 4]);
        assert_eq!(bytes.split_off(2).map(|b| b.len()), Some(2));
        assert_eq!(bytes.split_off(4).map(|b| b.len()), Some(0));
        assert!(bytes.split_off(5).is_none());
        assert_eq!(bytes.slice(1..3).map(|b| b.as_bytes()), Some(&[2u8, 3][..]));
        assert!(bytes.slice(1..9).is_none());
    }
}
