//! The [STAT](https://learn.microsoft.com/en-us/typography/opentype/spec/stat) table

use font_types::Tag;

/// 'STAT'
pub const TAG: Tag = Tag::new(b"STAT");

include!("../../generated/generated_stat.rs");
