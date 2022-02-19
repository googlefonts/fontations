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
