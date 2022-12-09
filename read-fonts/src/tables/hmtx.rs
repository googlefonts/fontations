//! The [hmtx (Horizontal Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx) table

use types::Tag;

/// 'hmtx'
pub const TAG: Tag = Tag::new(b"hmtx");

include!("../../generated/generated_hmtx.rs");
