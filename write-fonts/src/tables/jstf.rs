//! The [JSTF](https://learn.microsoft.com/en-us/typography/opentype/spec/jstf) table.

/// Positioning lookups embedded in JSTF, whose offsets are always 16-bit.
pub type JstfMax = super::gpos::OriginalPositionLookupList;

include!("../../generated/generated_jstf.rs");
