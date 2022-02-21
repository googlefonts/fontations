//! Offsets to tables

use crate::Uint24;

/// A trait for the different offset representations.
pub trait Offset {
    /// Returns this offsize as a `usize`, or `None` if it is `0`.
    fn non_null(self) -> Option<usize>;
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
