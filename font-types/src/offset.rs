//! Offsets to tables

use std::num::{NonZeroU16, NonZeroU32};

use crate::{ExactSized, FromBeBytes, Uint24};

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

        impl ExactSized for $name {
            const SIZE: usize = $bits / 8;
        }

        impl ExactSized for Option<$name> {
            const SIZE: usize = $bits / 8;
        }

        unsafe impl FromBeBytes<{ $bits / 8 }> for $name {
            type Error = NullOffset;
            fn read(bytes: [u8; $bits / 8]) -> Result<Self, Self::Error> {
                $name::new(FromBeBytes::read(bytes).unwrap()).ok_or(NullOffset)
            }
        }

        unsafe impl FromBeBytes<{ $bits / 8 }> for Option<$name> {
            type Error = crate::Never;
            fn read(bytes: [u8; $bits / 8]) -> Result<Self, Self::Error> {
                FromBeBytes::read(bytes).map($name::new)
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

/// An error type representing an unexpected `NULL` offset.
#[derive(Debug, Clone)]
pub struct NullOffset;

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

impl ExactSized for Option<Offset24> {
    const SIZE: usize = 3;
}

impl ExactSized for Offset24 {
    const SIZE: usize = 3;
}

unsafe impl FromBeBytes<3> for Offset24 {
    type Error = NullOffset;
    fn read(bytes: [u8; 3]) -> Result<Self, Self::Error> {
        Offset24::new(Uint24::read(bytes).unwrap().into()).ok_or(NullOffset)
    }
}

unsafe impl FromBeBytes<3> for Option<Offset24> {
    type Error = crate::Never;
    fn read(bytes: [u8; 3]) -> Result<Self, Self::Error> {
        Uint24::read(bytes).map(|val| Offset24::new(val.into()))
    }
}
