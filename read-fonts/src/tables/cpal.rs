//! The [CPAL](https://docs.microsoft.com/en-us/typography/opentype/spec/cpal) table

use font_types::Tag;

/// 'CPAL'
pub const TAG: Tag = Tag::new(b"CPAL");

include!("../../generated/generated_cpal.rs");
