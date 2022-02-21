//! the basic integer types

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
