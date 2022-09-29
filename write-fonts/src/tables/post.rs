//! The post table

include!("../../generated/generated_post.rs");

//TODO: I imagine we're going to need a builder for this

/// A string in the post table.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PostString(String);

impl std::ops::Deref for PostString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl AsRef<str> for PostString {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

#[cfg(feature = "parsing")]
fn parse_pstrings(bytes: Option<&[u8]>) -> Option<Vec<PostString>> {
    let mut bytes = bytes?;
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let (len, tail) = bytes.split_first().unwrap();
        if *len as usize > tail.len() {
            break;
        }
        let (this, tail) = tail.split_at(*len as usize);
        bytes = tail;
        let s = String::from_utf8_lossy(this).into_owned();
        result.push(PostString(s));
    }
    Some(result)
}

impl FontWrite for PostString {
    fn write_into(&self, writer: &mut TableWriter) {
        let len = self.0.len() as u8;
        len.write_into(writer);
        self.0.as_bytes().write_into(writer);
    }
}

impl PartialEq<&str> for PostString {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pstrings() {
        static DATA: &[u8] = &[
            0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f, // 5, h e l l o
            0x02, 0x68, 0x69, // 2, h i
            0x4, 0x68, 0x6f, 0x6c, 0x61, // 4, h o l a
        ];

        let pstrings = parse_pstrings(Some(DATA)).unwrap();
        assert_eq!(pstrings.as_slice(), ["hello", "hi", "hola"].as_slice());
    }
}
