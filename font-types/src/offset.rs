//! Offsets to tables

use crate::Uint24;

/// A trait for the different offset representations.
pub trait Offset: Sized {
    /// The length in bytes of this offset type.
    const SIZE: OffsetLen;

    /// Returns this offsize as a `usize`, or `None` if it is `0`.
    fn non_null(self) -> Option<usize>;
    fn read<'a, T: crate::FontRead<'a>>(self, bytes: &'a [u8]) -> Option<T> {
        self.non_null()
            .and_then(|off| bytes.get(off..))
            .and_then(T::read)
    }
}

/// A type that contains data referenced by offsets.
pub trait OffsetHost<'a> {
    /// Return a slice of bytes from which offsets may be resolved.
    ///
    /// This should be relative to the start of the host.
    fn bytes(&self) -> &'a [u8];

    /// Return the bytes for a given offset
    fn bytes_at_offset(&self, offset: impl Offset) -> &'a [u8] {
        offset
            .non_null()
            .and_then(|off| self.bytes().get(off..))
            .unwrap_or_default()
    }

    fn resolve_offset<T: crate::FontRead<'a>>(&self, offset: impl Offset) -> Option<T> {
        crate::FontRead::read(self.bytes_at_offset(offset))
    }
}

/// The byte length of some offset.
///
/// This is sort of redundant, but it is useful during compilation to have
/// some token type that represents a pending offset.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum OffsetLen {
    Offset16 = 2,
    Offset24 = 3,
    Offset32 = 4,
}

impl OffsetLen {
    /// The empty represntation of this offset
    pub fn null_bytes(self) -> &'static [u8] {
        match self {
            Self::Offset16 => &[0, 0],
            Self::Offset24 => &[0, 0, 0],
            Self::Offset32 => &[0, 0, 0, 0],
        }
    }
}

macro_rules! impl_offset {
    ($name:ident, $bits:literal, $rawty:ty) => {
        #[doc = concat!("A", stringify!($bits), "-bit offset to a table.")]
        ///
        /// Specific offset fields may or may not permit NULL values; however we
        /// assume that errors are possible, and expect the caller to handle
        /// the `None` case.
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name($rawty);

        impl $name {
            /// Create a new offset.
            pub fn new(raw: $rawty) -> Self {
                Self(raw)
            }
        }

        impl crate::raw::Scalar for $name {
            type Raw = <$rawty as crate::raw::Scalar>::Raw;
            fn from_raw(raw: Self::Raw) -> Self {
                let raw = <$rawty>::from_raw(raw);
                $name::new(raw)
            }

            fn to_raw(self) -> Self::Raw {
                self.0.to_raw()
            }
        }

        impl Offset for $name {
            const SIZE: OffsetLen = OffsetLen::$name;

            fn non_null(self) -> Option<usize> {
                let raw: u32 = self.0.into();
                if raw == 0 {
                    None
                } else {
                    Some(raw as usize)
                }
            }
        }
    };
}

impl_offset!(Offset16, 16, u16);
impl_offset!(Offset24, 24, Uint24);
impl_offset!(Offset32, 32, u32);
