//! The [os2](https://docs.microsoft.com/en-us/typography/opentype/spec/os2) table

use font_types::Tag;

/// 'os/2'
pub const TAG: Tag = Tag::new(b"OS/2");

include!("../../generated/generated_os2.rs");
