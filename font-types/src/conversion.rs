//! Reading (and eventually writing) to raw bytes

/// A trait for types that can be constructed from an array of big-endian bytes.
///
/// # Safety
///
/// This conversion must be infallible; that is, all possible raw bit patterns
/// must be valid forms of this type. (For instance, this trait should not be
/// implemented for [`Tag`](crate::Tag) because the data could contain invalid
/// bytes.)
//NOTE: questionable that this needs to be unsafe, but it also should not
//be implemented at all outside of this crate? We can have a higher-level
//trait for types that composites of scalars.
pub unsafe trait FromBeBytes<const N: usize>: Sized {
    fn from_be_bytes(raw: [u8; N]) -> Self;
}

macro_rules! impl_from_be {
    ($name:ident, $size:literal) => {
        unsafe impl FromBeBytes<$size> for $name {
            fn from_be_bytes(raw: [u8; $size]) -> Self {
                $name::from_be_bytes(raw)
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
        unsafe impl crate::FromBeBytes<$size> for $name {
            fn from_be_bytes(raw: [u8; $size]) -> Self {
                Self(crate::FromBeBytes::from_be_bytes(raw))
            }
        }
    };
}

// necessary fort this macro to be used elsewhere in the crate
pub(crate) use impl_from_be_by_proxy;
