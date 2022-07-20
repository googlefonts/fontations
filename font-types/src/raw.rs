//! types for working with raw big-endian bytes

/// A trait for font scalars.
///
/// This is an internal trait for encoding and decoding big-endian bytes.
///
/// You do not need to implement this trait directly; it is an implemention
/// detail of the [`BigEndian`] wrapper.
pub trait Scalar {
    /// The raw byte representation of this type.
    type Raw: Copy + zerocopy::Unaligned + zerocopy::FromBytes + zerocopy::AsBytes + AsRef<[u8]>;

    /// Create an instance of this type from raw big-endian bytes
    fn from_raw(raw: Self::Raw) -> Self;
    /// Encode this type as raw big-endian bytes
    fn to_raw(self) -> Self::Raw;
}

/// A trait for types that have a known size.
pub trait FixedSized: Sized {
    /// The number of bytes required to encode this type.
    const RAW_BYTE_LEN: usize;
}

pub trait ReadScalar: FixedSized {
    fn read(bytes: &[u8]) -> Option<Self>;
}

/// A wrapper around raw big-endian bytes for some type.
#[derive(Clone, Copy, PartialEq, Eq, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct BigEndian<T: Scalar>(pub(crate) T::Raw);

impl<T: Scalar> BigEndian<T> {
    /// Read a copy of this type from raw bytes.
    pub fn get(self) -> T {
        T::from_raw(self.0)
    }

    /// Set the value, overwriting the bytes.
    pub fn set(&mut self, value: T) {
        self.0 = value.to_raw();
    }

    /// Get the raw big-endian bytes.
    pub fn be_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<T: Scalar> From<T> for BigEndian<T> {
    #[inline]
    fn from(val: T) -> Self {
        BigEndian(val.to_raw())
    }
}

impl<T: Scalar + Default> Default for BigEndian<T> {
    fn default() -> Self {
        Self::from(T::default())
    }
}

impl<T: Scalar + Copy + PartialEq> PartialEq<T> for BigEndian<T> {
    fn eq(&self, other: &T) -> bool {
        self.get() == *other
    }
}

/// An internal macro for implementing the `RawType` trait.
#[macro_export]
macro_rules! newtype_scalar {
    ($name:ident, $raw:ty) => {
        impl crate::raw::Scalar for $name {
            type Raw = $raw;
            fn to_raw(self) -> $raw {
                self.0.to_raw()
            }

            fn from_raw(raw: $raw) -> Self {
                Self(crate::raw::Scalar::from_raw(raw))
            }
        }

        impl crate::raw::FixedSized for $name {
            const RAW_BYTE_LEN: usize = std::mem::size_of::<$raw>();
        }

        impl crate::raw::ReadScalar for $name {
            #[inline]
            fn read(bytes: &[u8]) -> Option<Self> {
                bytes
                    .get(..<Self as crate::FixedSized>::RAW_BYTE_LEN)
                    .map(|bytes| crate::raw::Scalar::from_raw(bytes.try_into().unwrap()))
            }
        }
    };
}

macro_rules! int_scalar {
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

        impl crate::raw::FixedSized for $ty {
            const RAW_BYTE_LEN: usize = std::mem::size_of::<$raw>();
        }

        impl crate::raw::ReadScalar for $ty {
            #[inline]
            fn read(bytes: &[u8]) -> Option<Self> {
                bytes
                    .get(..Self::RAW_BYTE_LEN)
                    .map(|bytes| crate::raw::Scalar::from_raw(bytes.try_into().unwrap()))
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

impl<T: std::fmt::Debug + Scalar + Copy> std::fmt::Debug for BigEndian<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

impl<T: std::fmt::Display + Scalar + Copy> std::fmt::Display for BigEndian<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.get().fmt(f)
    }
}
