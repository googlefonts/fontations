//! The [meta (Metadata)](https://docs.microsoft.com/en-us/typography/opentype/spec/meta) table

use std::fmt::Display;

include!("../../generated/generated_meta.rs");

pub const DLNG: Tag = Tag::new(b"dlng");
pub const SLNG: Tag = Tag::new(b"slng");

/// Metadata in the `meta` table, associated with some tag.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Metadata {
    /// For the 'dlng' and 'slng' tags
    ScriptLangTags(Vec<ScriptLangTag>),
    /// For other tags
    Other(Vec<u8>),
}

/// A ['ScriptLangTag'] value.
///
/// This is currently just a string and we do not perform any validation,
/// but we should do that (TK open issue)
///
/// [`ScriptLangTag`]: https://learn.microsoft.com/en-us/typography/opentype/spec/meta#scriptlangtag-values
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptLangTag(String);

/// An error for if a [`ScriptLangTag`] does not conform to the specification.
#[derive(Clone, Debug)]
#[non_exhaustive] // so we can flesh this out later without breaking anything
pub struct InvalidScriptLangTag;

impl ScriptLangTag {
    pub fn new(raw: String) -> Result<Self, InvalidScriptLangTag> {
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for InvalidScriptLangTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScriptLangTag was malformed")
    }
}

impl std::error::Error for InvalidScriptLangTag {}

impl DataMapRecord {
    fn validate_data_type(&self, ctx: &mut ValidationCtx) {
        if matches!(
            (self.tag, self.data.as_ref()),
            (SLNG | DLNG, Metadata::Other(_))
        ) {
            ctx.report("'slng' or 'dlng' tags use ScriptLangTag data");
        }
    }
}

impl FontWrite for Metadata {
    fn write_into(&self, writer: &mut TableWriter) {
        let len = match self {
            Metadata::ScriptLangTags(langs) => {
                let mut len = 0;
                for lang in langs {
                    if len > 0 {
                        b','.write_into(writer);
                        len += 1;
                    }
                    lang.0.as_bytes().write_into(writer);
                    len += lang.0.as_bytes().len();
                }
                len
            }
            Metadata::Other(vec) => {
                vec.write_into(writer);
                vec.len()
            }
        };

        let len: u32 = len.try_into().unwrap();
        len.write_into(writer);
    }
}

impl Validate for Metadata {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}

impl FromObjRef<read_fonts::tables::meta::Metadata<'_>> for Metadata {
    fn from_obj_ref(from: &read_fonts::tables::meta::Metadata<'_>, _: FontData) -> Self {
        match from {
            read_fonts::tables::meta::Metadata::ScriptLangTags(var_len_array) => {
                Self::ScriptLangTags(
                    var_len_array
                        .iter()
                        .flat_map(|x| {
                            x.ok()
                                .and_then(|x| ScriptLangTag::new(x.as_str().into()).ok())
                        })
                        .collect(),
                )
            }
            read_fonts::tables::meta::Metadata::Other(bytes) => Self::Other(bytes.to_vec()),
        }
    }
}

impl FromTableRef<read_fonts::tables::meta::Metadata<'_>> for Metadata {}

// Note: This is required because of generated trait bounds, but we don't really
// want to use it because we want our metadata to match our tag...
impl Default for Metadata {
    fn default() -> Self {
        Metadata::ScriptLangTags(Vec::new())
    }
}

impl FromObjRef<read_fonts::tables::meta::DataMapRecord> for DataMapRecord {
    fn from_obj_ref(obj: &read_fonts::tables::meta::DataMapRecord, offset_data: FontData) -> Self {
        let data = obj
            .data(offset_data)
            .map(|meta| meta.to_owned_table())
            .unwrap_or_else(|_| match obj.tag() {
                DLNG | SLNG => Metadata::ScriptLangTags(Vec::new()),
                _ => Metadata::Other(Vec::new()),
            });
        DataMapRecord {
            tag: obj.tag(),
            data: OffsetMarker::new(data),
        }
    }
}
