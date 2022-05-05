//! The [hmtx (Horizontal Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx) table

#[path = "../../generated/generated_hmtx.rs"]
mod generated;

pub use generated::*;

use font_types::Tag;

pub const TAG: Tag = Tag::new(b"hmtx");
