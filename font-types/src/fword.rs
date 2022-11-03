//! 16-bit signed and unsigned font-units

/// 16-bit signed quantity in font design units.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct FWord(i16);

/// 16-bit unsigned quantity in font design units.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct UfWord(u16);

impl FWord {
    pub fn new(raw: i16) -> Self {
        Self(raw)
    }

    pub fn to_i16(self) -> i16 {
        self.0
    }

    /// The representation of this number as a big-endian byte array.
    pub fn to_be_bytes(self) -> [u8; 2] {
        self.0.to_be_bytes()
    }
}

impl UfWord {
    pub fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub fn to_u16(self) -> u16 {
        self.0
    }

    /// The representation of this number as a big-endian byte array.
    pub fn to_be_bytes(self) -> [u8; 2] {
        self.0.to_be_bytes()
    }
}

impl std::fmt::Display for FWord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for UfWord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

crate::newtype_scalar!(FWord, [u8; 2]);
crate::newtype_scalar!(UfWord, [u8; 2]);
//TODO: we can add addition/etc as needed
