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
