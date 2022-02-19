use crate::integers::RawU32;

/// A legacy 16/16 version encoding

/// Packed 32-bit value with major and minor version numbers.
///
/// This is a legacy type with an unusual representation. See [the spec][] for
/// additional details.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version16Dot16(u32);

/// A raw (big-endian) [`Version16Dot16`].
#[derive(Debug, Clone, Copy, zerocopy::Unaligned, zerocopy::FromBytes)]
#[repr(transparent)]
pub struct RawVersion16Dot16(RawU32);

impl Version16Dot16 {
    /// Create a new version with the provided major and minor parts.
    ///
    /// The minor version must be in the range 0..=9.
    ///
    /// # Panics
    ///
    /// Panics if `minor > 9`.
    pub const fn new(major: u16, minor: u16) -> Self {
        assert!(minor < 10, "minor version must be in the range [0, 9)");
        let version = (major as u32) << 16 | (minor as u32) << 12;
        Version16Dot16(version)
    }

    /// Return the separate major & minor version numbers.
    pub const fn to_major_minor(self) -> (u16, u16) {
        let major = (self.0 >> 16) as u16;
        let minor = ((self.0 & 0xFFFF) >> 12) as u16;
        (major, minor)
    }
}

impl crate::RawType for RawVersion16Dot16 {
    type Cooked = Version16Dot16;
    fn get(self) -> Version16Dot16 {
        Version16Dot16(self.0.get())
    }
}

impl std::fmt::Debug for Version16Dot16 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Version16Dot16({:08x})", self.0)
    }
}

impl std::fmt::Display for Version16Dot16 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (major, minor) = self.to_major_minor();
        write!(f, "{}.{}", major, minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_smoke_test() {
        assert_eq!(Version16Dot16(0x00005000).to_major_minor(), (0, 5));
        assert_eq!(Version16Dot16(0x00011000).to_major_minor(), (1, 1));
        assert_eq!(Version16Dot16::new(0, 5).0, 0x00005000);
        assert_eq!(Version16Dot16::new(1, 1).0, 0x00011000);
    }
}
