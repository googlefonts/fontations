//! the [hhea (Horizontal Header)](https://docs.microsoft.com/en-us/typography/opentype/spec/hhea) table

#[path = "../../generated/generated_hhea.rs"]
mod generated;

pub use generated::*;

use font_types::Tag;

pub const TAG: Tag = Tag::new(b"hhea");
