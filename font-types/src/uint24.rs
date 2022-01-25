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
}

impl From<Uint24> for u32 {
    fn from(src: Uint24) -> u32 {
        src.0
    }
}

impl super::FromBeBytes<3> for super::Uint24 {
    type Error = crate::Never;
    fn read(raw: [u8; 3]) -> Result<Self, Self::Error> {
        Ok(Self(
            (raw[0] as u32) << 16 | (raw[1] as u32) << 8 | raw[2] as u32,
        ))
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
}
