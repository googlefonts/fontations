//! Reading (and eventually writing) to raw bytes

/// A trait for types that that can be constructed from raw bytes.
///
/// This trait is generic over the length of the bytes; this delegates
/// responsibility for bounds checking (or not) to the caller.
///
/// It is the responsibility of the implementor to know how these bytes should
/// be interpreted; for instance `[u8; 4]` could be either two u16s or a single
/// i32.
pub trait FromBeBytes<const N: usize>: Sized {
    /// An error describing casees where the input bytes do not represent a valid value.
    type Error;
    /// Convert the provided big-endian byte array into a value of this type.
    fn read(bytes: [u8; N]) -> Result<Self, Self::Error>;
}

/// An error type for impossible errors.
pub enum Never {}

macro_rules! impl_from_be {
    ($name:ident, $size:literal) => {
        impl FromBeBytes<$size> for $name {
            type Error = Never;
            fn read(raw: [u8; $size]) -> Result<Self, Never> {
                Ok($name::from_be_bytes(raw))
            }
        }
    };
}

impl_from_be!(u8, 1);
impl_from_be!(i8, 1);
impl_from_be!(u16, 2);
impl_from_be!(i16, 2);
impl_from_be!(u32, 4);
impl_from_be!(i32, 4);
impl_from_be!(i64, 8);
// other impls are in their respective modules

#[macro_export]
macro_rules! impl_from_be_by_proxy {
    ($name:ident, $size:literal) => {
        impl crate::FromBeBytes<$size> for $name {
            type Error = crate::Never;
            fn read(raw: [u8; $size]) -> Result<Self, Self::Error> {
                crate::FromBeBytes::read(raw).map(Self)
            }
        }
    };
}

// necessary fort this macro to be used elsewhere in the crate
pub(crate) use impl_from_be_by_proxy;

impl std::fmt::Debug for Never {
    fn fmt(&self, _: &mut std::fmt::Formatter) -> std::fmt::Result {
        Ok(())
    }
}

impl std::fmt::Display for Never {
    fn fmt(&self, _: &mut std::fmt::Formatter) -> std::fmt::Result {
        Ok(())
    }
}

#[cfg(any(feature = "std", test))]
impl std::error::Error for Never {}
