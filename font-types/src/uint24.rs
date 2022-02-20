/// 24-bit unsigned integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uint24(u32);

impl Uint24 {
    /// The smallest value that can be represented by this integer type.
    pub const MIN: Self = Uint24(0);

    /// The largest value that can be represented by this integer type.
    pub const MAX: Self = Uint24(0xffffff);

    /// Create from a u32. Saturates on overflow.
    pub fn new(raw: u32) -> Uint24 {
        let overflow = raw > Self::MAX.0;
        let raw = raw * !overflow as u32 + Self::MAX.0 * overflow as u32;
        Uint24(raw)
    }

    /// Create from a u32, returning `None` if the value overflows.
    pub const fn checked_new(raw: u32) -> Option<Uint24> {
        if raw > Self::MAX.0 {
            None
        } else {
            Some(Uint24(raw))
        }
    }

    pub fn to_be_bytes(self) -> [u8; 3] {
        let bytes = self.0.to_be_bytes();
        [bytes[1], bytes[2], bytes[3]]
    }

    pub fn from_be_bytes(bytes: [u8; 3]) -> Self {
        Uint24::new((bytes[0] as u32) << 16 | (bytes[1] as u32) << 8 | bytes[2] as u32)
    }
}

impl From<Uint24> for u32 {
    fn from(src: Uint24) -> u32 {
        src.0
    }
}

/// An unaligned big-endian unsigned 24-bit integer.
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawU24([u8; 3]);

impl crate::RawType for RawU24 {
    type Cooked = Uint24;
    fn get(self) -> Uint24 {
        Uint24::new((self.0[0] as u32) << 16 | (self.0[1] as u32) << 8 | self.0[2] as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor() {
        assert_eq!(Uint24::MAX, Uint24::new(u32::MAX));
        assert!(Uint24::checked_new(u32::MAX).is_none())
    }

    #[test]
    fn be_bytes() {
        let bytes = [0xff, 0b10101010, 0b11001100];
        let val = Uint24::from_be_bytes(bytes);
        assert_eq!(val.to_be_bytes(), bytes);
    }
}
