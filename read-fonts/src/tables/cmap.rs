//! The [cmap](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap) table

use font_types::Tag;

/// 'cmap'
pub const TAG: Tag = Tag::new(b"cmap");

include!("../../generated/generated_cmap.rs");
