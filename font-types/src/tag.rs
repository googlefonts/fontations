use std::{
    borrow::Borrow,
    fmt::{Display, Formatter},
    ops::Deref,
    str::FromStr,
};

//FIXME: this needs to  be rethought, we need to safely handle invalid data

/// An OpenType tag.
///
/// A tag is a 4-byte array where each byte is in the printable ascii range
/// (0x20..=0x7E).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    zerocopy::Unaligned,
    zerocopy::FromBytes,
)]
#[repr(transparent)]
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
            if i <= 0x20 || i > 0x7e {
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

    /// This tag as raw bytes.
    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    /// This tag as a `&str`.
    pub fn as_str(&self) -> &str {
        // safety: tag can only be constructed from valid utf-8
        unsafe { std::str::from_utf8_unchecked(&self.0) }
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

// Tag is stored as [u8; 4] so encoding is correct regardless of endianness
impl crate::RawType for Tag {
    type Cooked = Tag;
    fn get(self) -> Tag {
        self
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Tag {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        unsafe { std::str::from_utf8_unchecked(&self.0) }
    }
}

impl Borrow<[u8; 4]> for Tag {
    fn borrow(&self) -> &[u8; 4] {
        &self.0
    }
}

impl Borrow<str> for Tag {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<[u8; 4]> for Tag {
    fn eq(&self, other: &[u8; 4]) -> bool {
        &self.0 == other
    }
}

impl PartialEq<str> for Tag {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
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
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
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
}
