//! The [maxp](https://docs.microsoft.com/en-us/typography/opentype/spec/maxp) table

#[path = "../../generated/generated_maxp.rs"]
mod generated;

pub use generated::*;

use font_types::Tag;

pub const TAG: Tag = Tag::new(b"maxp");
