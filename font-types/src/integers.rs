//! the basic integer types

/// An unaligned big-endian signed 16-bit integer.
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawI16([u8; 2]);

/// An unaligned big-endian unsigned 16-bit integer.
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawU16([u8; 2]);

/// An unaligned big-endian signed 32-bit integer.
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawI32([u8; 4]);

/// An unaligned big-endian unsigned 32-bit integer.
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawU32([u8; 4]);

macro_rules! int_raw_type {
    ($ident:ty) => {
        impl crate::RawType for $ident {
            type Cooked = $ident;
            fn get(self) -> $ident {
                self
            }
        }
    };
    ($raw:ty, $cooked:ty) => {
        impl crate::RawType for $raw {
            type Cooked = $cooked;
            fn get(self) -> $cooked {
                <$cooked>::from_be_bytes(self.0)
            }
        }
    };
}

int_raw_type!(RawI16, i16);
int_raw_type!(RawU16, u16);
int_raw_type!(RawI32, i32);
int_raw_type!(RawU32, u32);
int_raw_type!(u8);
int_raw_type!(i8);

macro_rules! int_scalar {
    ($ident:ty) => {
        impl crate::raw::Scalar for $ident {
            type Raw = $ident;
            fn to_raw(self) -> $ident {
                self
            }
            fn from_raw(raw: $ident) -> $ident {
                raw
            }
        }
    };
    ($ty:ty, $raw:ty) => {
        impl crate::raw::Scalar for $ty {
            type Raw = $raw;
            fn to_raw(self) -> $raw {
                self.to_be_bytes()
            }

            fn from_raw(raw: $raw) -> $ty {
                Self::from_be_bytes(raw)
            }
        }
    };
}

int_scalar!(u8, [u8; 1]);
int_scalar!(i8, [u8; 1]);
int_scalar!(u16, [u8; 2]);
int_scalar!(i16, [u8; 2]);
int_scalar!(u32, [u8; 4]);
int_scalar!(i32, [u8; 4]);
int_scalar!(i64, [u8; 8]);
int_scalar!(crate::Uint24, [u8; 3]);
