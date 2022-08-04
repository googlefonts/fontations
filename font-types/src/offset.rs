//! Offsets to tables

use crate::{FixedSized, Uint24};

/// A trait for the different offset representations.
pub trait Offset: FixedSized + Copy {
    /// Returns this offsize as a `usize`, or `None` if it is `0`.
    fn non_null(self) -> Option<usize>;
    /// the bytes that encode a null value of for this offset
    fn null_bytes() -> &'static [u8];
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

            /// Return `true` if this offset is null.
            pub fn is_null(self) -> bool {
                let as_u32: u32 = self.0.into();
                as_u32 == 0
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
            fn non_null(self) -> Option<usize> {
                let raw: u32 = self.0.into();
                if raw == 0 {
                    None
                } else {
                    Some(raw as usize)
                }
            }

            /// A raw byte slice of the same length as this offset.
            fn null_bytes() -> &'static [u8] {
                [0u8; <Self as crate::FixedSized>::RAW_BYTE_LEN].as_slice()
            }
        }

        // useful for debugging
        impl PartialEq<u32> for $name {
            fn eq(&self, other: &u32) -> bool {
                self.non_null().unwrap_or_default() as u32 == *other
            }
        }
    };
}

impl_offset!(Offset16, 16, u16);
impl_offset!(Offset24, 24, Uint24);
impl_offset!(Offset32, 32, u32);
