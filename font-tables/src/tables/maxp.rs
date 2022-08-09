//! The [maxp](https://docs.microsoft.com/en-us/typography/opentype/spec/maxp) table

use font_types::Tag;

/// 'maxp'
pub const TAG: Tag = Tag::new(b"maxp");

include!("../../generated/generated_maxp.rs");
