//! the [hhea (Horizontal Header)](https://docs.microsoft.com/en-us/typography/opentype/spec/hhea) table

use font_types::Tag;

/// 'hhea'
pub const TAG: Tag = Tag::new(b"hhea");

include!("../../generated/generated_hhea.rs");
