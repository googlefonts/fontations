//! Offsets to tables

use crate::Uint24;

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
            #[inline]
            pub fn new(raw: $rawty) -> Self {
                Self(raw)
            }

            /// Return `true` if this offset is null.
            #[inline]
            pub fn is_null(self) -> bool {
                self.to_u32() == 0
            }

            #[inline]
            pub fn to_u32(self) -> u32 {
                self.0.into()
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

        // useful for debugging
        impl PartialEq<u32> for $name {
            fn eq(&self, other: &u32) -> bool {
                self.to_u32() == *other
            }
        }
    };
}

impl_offset!(Offset16, 16, u16);
impl_offset!(Offset24, 24, Uint24);
impl_offset!(Offset32, 32, u32);
