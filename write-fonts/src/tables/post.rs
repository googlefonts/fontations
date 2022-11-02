//! The post table

include!("../../generated/generated_post.rs");

//TODO: I imagine we're going to need a builder for this

/// A string in the post table.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PString(String);

impl std::ops::Deref for PString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl AsRef<str> for PString {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<'a> FromObjRef<read_fonts::tables::post::PString<'a>> for PString {
    fn from_obj_ref(from: &read_fonts::tables::post::PString<'a>, _: FontData) -> Self {
        PString(from.as_str().to_owned())
    }
}

impl FontWrite for PString {
    fn write_into(&self, writer: &mut TableWriter) {
        let len = self.0.len() as u8;
        len.write_into(writer);
        self.0.as_bytes().write_into(writer);
    }
}

impl PartialEq<&str> for PString {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn roundtrip() {
        use super::*;
        use read_fonts::test_data::post as test_data;

        let table = Post::read(test_data::SIMPLE).unwrap();
        let dumped = crate::dump_table(&table).unwrap();
        assert_eq!(test_data::SIMPLE.as_ref(), &dumped);
    }
}
