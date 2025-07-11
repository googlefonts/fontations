//! Types for working with raw little-endian data, currently only used for
//! Apple's `hvgl` and `hvpm` tables.

use crate::{BytesWrapper, FixedSize, MajorMinor, Offset16, Offset24, Offset32, Scalar, Uint24};

/// A trait for little-endian font scalars.
///
/// The vast majority of OpenType data is stored as big-endian--see [`Scalar`]
/// for this trait's big-endian counterpart.
pub trait ScalarLE: Scalar {
    /// Create an instance of this type from raw little-endian bytes
    fn from_raw_le(raw: <Self as Scalar>::Raw) -> Self;

    /// Encode this type as raw little-endian bytes
    fn to_raw_le(self) -> <Self as Scalar>::Raw;

    /// Attempt to read a scalar from a slice.
    ///
    /// This will always succeed if `slice.len() == Self::RAW_BYTE_LEN`, and will
    /// always return `None` otherwise.
    fn read_le(slice: &[u8]) -> Option<Self> {
        crate::raw::sealed::BeByteArray::from_slice(slice).map(Self::from_raw_le)
    }
}

/// A wrapper around raw little-endian bytes for some type.
///
/// Little-endian data is very rare in font files, but used by some
/// Apple-specific tables. See [`crate::BigEndian`] for this type's big-endian
/// counterpart.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
pub struct LittleEndian<T: ScalarLE>(pub(crate) T::Raw);

// # SAFETY:
//
// `LittleEndian<T>` has the bound `T: Scalar`, and contains only a single value,
// `<T as Scalar>::Raw` which is only ever a byte array.
#[cfg(feature = "bytemuck")]
unsafe impl<T> bytemuck::Zeroable for LittleEndian<T> where T: ScalarLE + Copy {}
#[cfg(feature = "bytemuck")]
unsafe impl<T> bytemuck::AnyBitPattern for LittleEndian<T> where T: ScalarLE + Copy + 'static {}

impl<T: ScalarLE> BytesWrapper for LittleEndian<T> {
    type Inner = T;
    /// Attempt to construct a new raw value from this slice.
    ///
    /// This will fail if `slice.len() != T::RAW_BYTE_LEN`.
    fn from_slice(slice: &[u8]) -> Option<Self> {
        crate::raw::sealed::BeByteArray::from_slice(slice).map(Self)
    }

    /// Convert this raw type to its native representation.
    #[inline(always)]
    fn get(&self) -> T {
        T::from_raw(self.0)
    }

    /// Set the value, overwriting the bytes.
    fn set(&mut self, value: T) {
        self.0 = value.to_raw();
    }
}

impl<T: ScalarLE> LittleEndian<T> {
    /// construct a new `LittleEndian<T>` from raw bytes
    pub fn new(raw: T::Raw) -> LittleEndian<T> {
        LittleEndian(raw)
    }

    /// Get the raw little-endian bytes.
    pub fn le_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<T: ScalarLE> From<T> for LittleEndian<T> {
    #[inline]
    fn from(val: T) -> Self {
        LittleEndian(val.to_raw_le())
    }
}

impl<T: ScalarLE + Default> Default for LittleEndian<T> {
    fn default() -> Self {
        Self::from(T::default())
    }
}

// NOTE: do to the orphan rules, we cannot impl the inverse of this, e.g.
// impl<T> PartialEq<LittleEndian<T>> for T (<https://doc.rust-lang.org/error_codes/E0210.html>)
impl<T: ScalarLE + Copy + PartialEq> PartialEq<T> for LittleEndian<T> {
    fn eq(&self, other: &T) -> bool {
        self.get() == *other
    }
}

impl<T: ScalarLE + Copy + PartialOrd + PartialEq> PartialOrd for LittleEndian<T>
where
    <T as Scalar>::Raw: PartialEq,
{
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.get().partial_cmp(&other.get())
    }
}

impl<T: ScalarLE + Copy + Ord + Eq> Ord for LittleEndian<T>
where
    <T as Scalar>::Raw: Eq,
{
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.get().cmp(&other.get())
    }
}

impl<T: FixedSize + ScalarLE> FixedSize for LittleEndian<T> {
    const RAW_BYTE_LEN: usize = T::RAW_BYTE_LEN;
}

macro_rules! int_scalar {
    ($ty:ty) => {
        impl crate::raw_le::ScalarLE for $ty {
            fn to_raw_le(self) -> Self::Raw {
                self.to_le_bytes()
            }

            #[inline(always)]
            fn from_raw_le(raw: Self::Raw) -> $ty {
                Self::from_le_bytes(raw)
            }
        }
    };
}

int_scalar!(u8);
int_scalar!(i8);
int_scalar!(u16);
int_scalar!(i16);
int_scalar!(u32);
int_scalar!(i32);
int_scalar!(i64);

impl ScalarLE for Uint24 {
    fn from_raw_le(raw: Self::Raw) -> Self {
        Uint24::new(((raw[2] as u32) << 16) | ((raw[1] as u32) << 8) | raw[0] as u32)
    }

    fn to_raw_le(self) -> Self::Raw {
        let bytes = self.to_u32().to_le_bytes();
        [bytes[0], bytes[1], bytes[2]]
    }
}

impl ScalarLE for Offset16 {
    fn from_raw_le(raw: Self::Raw) -> Self {
        Self::new(u16::from_raw_le(raw))
    }

    fn to_raw_le(self) -> Self::Raw {
        (self.to_u32() as u16).to_raw_le()
    }
}

impl ScalarLE for Offset24 {
    fn from_raw_le(raw: Self::Raw) -> Self {
        Self::new(Uint24::from_raw_le(raw))
    }

    fn to_raw_le(self) -> Self::Raw {
        Uint24::new(self.to_u32()).to_raw_le()
    }
}

impl ScalarLE for Offset32 {
    fn from_raw_le(raw: Self::Raw) -> Self {
        Self::new(u32::from_raw_le(raw))
    }

    fn to_raw_le(self) -> Self::Raw {
        self.to_u32().to_raw_le()
    }
}

impl ScalarLE for MajorMinor {
    fn from_raw_le(raw: Self::Raw) -> Self {
        let major = u16::from_le_bytes([raw[0], raw[1]]);
        let minor = u16::from_le_bytes([raw[2], raw[3]]);
        Self { major, minor }
    }

    fn to_raw_le(self) -> Self::Raw {
        let [a, b] = self.major.to_le_bytes();
        let [c, d] = self.minor.to_le_bytes();
        [a, b, c, d]
    }
}
