//! Offsets to tables

use std::num::{NonZeroU16, NonZeroU32};

use crate::Uint24;

macro_rules! impl_offset {
    ($name:ident, $bits:literal, $ty:ty, $rawty:ty) => {
        #[doc = concat!("A", stringify!($bits), "-bit offset to a table.")]
        ///
        /// Specific offset fields may or may not permit NULL values. For that
        /// reason, you may specific a field as `Option<Offset>` and have the
        /// `None` case represent NULL, or you can use a non-optional offset
        /// and have NULL be treated as an error.
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name($ty);

        impl $name {
            /// Create a new offset.
            pub fn new(raw: $rawty) -> Option<Self> {
                <$ty>::new(raw).map(Self)
            }

            /// Return the raw integer value of this offset
            pub fn to_raw(self) -> $rawty {
                self.0.get()
            }
        }

        impl crate::raw::Scalar for Option<$name> {
            type Raw = <$rawty as crate::raw::Scalar>::Raw;
            fn from_raw(raw: Self::Raw) -> Self {
                let raw = <$rawty>::from_raw(raw);
                $name::new(raw)
            }

            fn to_raw(self) -> Self::Raw {
                self.map(|x| x.0.get()).unwrap_or_default().to_raw()
            }
        }
    };
}

impl_offset!(Offset16, 16, NonZeroU16, u16);
impl_offset!(Offset32, 32, NonZeroU32, u32);

/// A 24-bit offset to a table.
///
/// reason, you may specific a field as `Option<Offset>` and have the
/// `None` case represent NULL, or you can use a non-optional offset
/// and have NULL be treated as an error.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset24(NonZeroU32);

impl Offset24 {
    /// Create a new offset.
    pub fn new(raw: u32) -> Option<Self> {
        NonZeroU32::new(raw).map(Self)
    }

    /// Return the raw integer value of this offset
    pub fn to_raw(self) -> u32 {
        self.0.get()
    }
}

impl crate::raw::Scalar for Option<Offset24> {
    type Raw = [u8; 3];
    fn from_raw(raw: Self::Raw) -> Self {
        Offset24::new(Uint24::from_raw(raw).into())
    }

    fn to_raw(self) -> Self::Raw {
        Uint24::new(self.map(|x| x.0.get()).unwrap_or_default()).to_raw()
    }
}
