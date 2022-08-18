use std::{
    borrow::Borrow,
    fmt::{Debug, Display, Formatter},
    str::FromStr,
};

/// An OpenType tag.
///
/// A tag is a 4-byte array where each byte is in the printable ascii range
/// (0x20..=0x7E).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag([u8; 4]);

impl Tag {
    /// Generate a `Tag` from a string literal, verifying it conforms to the
    /// OpenType spec.
    ///
    /// The argument must be a non-empty string literal. Containing at most four
    /// characters in the printable ascii range, `0x20..=0x7E`.
    ///
    /// If the input has fewer than four characters, it will be padded with the space
    /// (' ', `0x20`) character.
    ///
    /// # Panics
    ///
    /// This method panics if the tag is not valid per the requirements above.
    pub const fn new(src: &[u8]) -> Tag {
        assert!(
            !src.is_empty() && src.len() < 5,
            "input must be 1-4 bytes in length"
        );
        let mut raw = [b' '; 4];
        let mut i = 0;
        while i < src.len() {
            if src[i] <= 0x20 || src[i] > 0x7e {
                panic!("all bytes must be in range (0x20, 0x7E)");
            }
            raw[i] = src[i];
            i += 1;
        }
        Tag(raw)
    }

    /// Attempt to create a `Tag` from raw bytes.
    ///
    /// The argument may be a slice of bytes, a `&str`, or any other type that
    /// impls `AsRef<[u8]>`.
    ///
    /// The slice must contain between 1 and 4 bytes, each in the printable
    /// ascii range (`0x20..=0x7E`).
    ///
    /// If the input has fewer than four bytes, spaces will be appended.
    pub fn new_checked(src: &[u8]) -> Result<Self, InvalidTag> {
        if src.is_empty() || src.len() > 4 {
            return Err(InvalidTag::InvalidLength(src.len()));
        }
        if let Some(pos) = src.iter().position(|b| !(0x20..=0x7E).contains(b)) {
            let byte = src[pos];

            return Err(InvalidTag::InvalidByte { pos, byte });
        }
        let mut out = [b' '; 4];
        out[..src.len()].copy_from_slice(src);

        // I think this is all fine but I'm also frequently wrong, so
        debug_assert!(std::str::from_utf8(&out).is_ok());
        Ok(Tag(out))
    }

    // for symmetry with integer types / other things we encode/decode
    /// Return the memory representation of this tag.
    pub fn to_be_bytes(self) -> [u8; 4] {
        self.0
    }

    /// Create a tag from raw big-endian bytes.
    ///
    /// Prefer to use [`Tag::new`] (in const contexts) or [`Tag::new_checked`]
    /// when creating a `Tag`.
    ///
    /// This does not check the input, and is only intended to be used during
    /// parsing, where invalid inputs are accepted.
    pub fn from_be_bytes(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }
}

/// An error representing an invalid tag.
#[derive(Clone, Debug)]
pub enum InvalidTag {
    InvalidLength(usize),
    InvalidByte { pos: usize, byte: u8 },
}

impl FromStr for Tag {
    type Err = InvalidTag;

    fn from_str(src: &str) -> Result<Self, Self::Err> {
        Tag::new_checked(src.as_bytes())
    }
}

impl crate::raw::Scalar for Tag {
    type Raw = [u8; 4];

    fn to_raw(self) -> Self::Raw {
        self.to_be_bytes()
    }

    fn from_raw(raw: Self::Raw) -> Self {
        Self::from_be_bytes(raw)
    }
}

impl Borrow<[u8; 4]> for Tag {
    fn borrow(&self) -> &[u8; 4] {
        &self.0
    }
}

impl PartialEq<[u8; 4]> for Tag {
    fn eq(&self, other: &[u8; 4]) -> bool {
        &self.0 == other
    }
}

impl PartialEq<str> for Tag {
    fn eq(&self, other: &str) -> bool {
        self.0 == other.as_bytes()
    }
}

impl PartialEq<&str> for Tag {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl PartialEq<&[u8]> for Tag {
    fn eq(&self, other: &&[u8]) -> bool {
        self.0.as_ref() == *other
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        // a dumb no-std way of ensuring this string is valid utf-8
        let mut bytes = [b'-'; 4];
        for (i, b) in self.0.iter().enumerate() {
            if b.is_ascii() {
                bytes[i] = *b;
            }
        }
        Display::fmt(&std::str::from_utf8(&bytes).unwrap(), f)
    }
}

impl Display for InvalidTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            InvalidTag::InvalidByte { pos, byte } => {
                write!(f, "Invalid byte 0x{:X} at index {pos}", byte)
            }
            InvalidTag::InvalidLength(len) => write!(f, "Invalid length ({len})"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Tag {}

impl Debug for Tag {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut dbg = f.debug_tuple("Tag");
        let mut bytes = [b'-'; 4];
        for (i, b) in self.0.iter().enumerate() {
            if b.is_ascii() {
                bytes[i] = *b;
            }
        }
        dbg.field(&std::str::from_utf8(&bytes).unwrap());
        dbg.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        Tag::new(b"head");
        assert!(Tag::new_checked(b"").is_err());
        assert!(Tag::new_checked(b"a").is_ok());
        assert!(Tag::new_checked(b"ab").is_ok());
        assert!(Tag::new_checked(b"abc").is_ok());
        assert!(Tag::new_checked(b"abcd").is_ok());
        assert!(Tag::new_checked(b"abcde").is_err());

        // ascii only:
        assert!(Tag::new_checked(&[0x19]).is_err());
        assert!(Tag::new_checked(&[0x20]).is_ok());
        assert!(Tag::new_checked(&[0x7E]).is_ok());
        assert!(Tag::new_checked(&[0x7F]).is_err());
    }

    #[test]
    #[should_panic]
    fn name() {
        let _ = Tag::new(&[0x19, 0x69]);
    }
}
