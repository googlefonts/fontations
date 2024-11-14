//! The [meta (Metadata)](https://docs.microsoft.com/en-us/typography/opentype/spec/meta) table

include!("../../generated/generated_meta.rs");

pub const DLNG: Tag = Tag::new(b"dlng");
pub const SLNG: Tag = Tag::new(b"slng");

/// Data stored in the 'meta' table.
pub enum Metadata<'a> {
    /// Used for the 'dlng' and 'slng' metadata
    ScriptLangTags(VarLenArray<'a, LangScriptTag<'a>>),
    /// Other metadata, which may exist in certain apple fonts
    Other(&'a [u8]),
}

impl ReadArgs for Metadata<'_> {
    type Args = (Tag, u32);
}

impl<'a> FontReadWithArgs<'a> for Metadata<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        let (tag, len) = *args;
        let data = data.slice(0..len as usize).ok_or(ReadError::OutOfBounds)?;
        if [DLNG, SLNG].contains(&tag) {
            VarLenArray::read(data).map(Metadata::ScriptLangTags)
        } else {
            Ok(Metadata::Other(data.as_bytes()))
        }
    }
}

pub struct LangScriptTag<'a>(&'a str);

impl<'a> LangScriptTag<'a> {
    pub fn as_str(&self) -> &'a str {
        self.0
    }
}

impl AsRef<str> for LangScriptTag<'_> {
    fn as_ref(&self) -> &str {
        self.0
    }
}

#[cfg(feature = "std")]
impl From<LangScriptTag<'_>> for String {
    fn from(value: LangScriptTag<'_>) -> Self {
        value.0.into()
    }
}

impl VarSize for LangScriptTag<'_> {
    type Size = u32;

    fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
        let bytes = data.split_off(pos)?.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let end = data
            .as_bytes()
            .iter()
            .position(|b| *b == b',')
            .map(|pos| pos + 1) // include comma
            .unwrap_or(bytes.len());
        Some(end)
    }
}

impl<'a> FontRead<'a> for LangScriptTag<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        std::str::from_utf8(data.as_bytes())
            .map_err(|_| ReadError::MalformedData("LangScriptTag must be utf8"))
            .map(|s| LangScriptTag(s.trim_matches(',')))
    }
}
