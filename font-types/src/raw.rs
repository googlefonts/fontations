//! Raw font types: unaligned big-endian bytes.

/// Raw big-endian bytes.
///
/// This trait is for conversion from raw bytes in the font file to the native
/// types used in most of the API.
pub trait RawType: zerocopy::Unaligned + zerocopy::FromBytes {
    type Cooked;
    fn get(self) -> Self::Cooked;
}

/// An internal macro for implementing the `RawType` trait.
#[macro_export]
macro_rules! newtype_raw_type {
    ($name:ident, $cooked:ty, $from:ty) => {
        impl crate::raw::RawType for $name {
            type Cooked = $cooked;
            fn get(self) -> $cooked {
                <$from>::new(self.0.get()).into()
            }
        }
    };

    ($name:ident, $cooked:ty) => {
        crate::newtype_raw_type!($name, $cooked, $cooked);
    };
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
    };
}

/// A trait for font scalars.
///
/// This is an internal trait for encoding and decoding big-endian bytes.
///
/// You do not need to implement this trait directly; it is an implemention
/// detail of the [`BigEndian`] wrapper.
pub trait Scalar {
    /// The raw byte representation of this type.
    type Raw: zerocopy::Unaligned + zerocopy::FromBytes + zerocopy::AsBytes;

    /// The size of the raw type. Essentially an alias for `std::mem::size_of`.
    //TODO: remove this probably
    const SIZE: usize = std::mem::size_of::<Self::Raw>();

    /// Create an instance of this type from raw big-endian bytes
    fn from_raw(raw: Self::Raw) -> Self;
    /// Encode this type as raw big-endian bytes
    fn to_raw(self) -> Self::Raw;
}

/// A wrapper around raw big-endian bytes for some type.
#[derive(Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct BigEndian<T: Scalar>(T::Raw);

impl<T: Scalar> BigEndian<T> {
    /// Read a copy of this type from raw bytes.
    pub fn get(self) -> T {
        T::from_raw(self.0)
    }

    /// Set the value, overwriting the bytes.
    pub fn set(&mut self, value: T) {
        self.0 = value.to_raw();
    }
}
