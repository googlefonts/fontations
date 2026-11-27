//! The [JSTF](https://learn.microsoft.com/en-us/typography/opentype/spec/jstf) table.

/// Positioning lookups embedded in JSTF, whose offsets are always 16-bit.
pub type JstfMax<'a> = super::gpos::OriginalPositionLookupList<'a>;

include!("../../generated/generated_jstf.rs");

impl<'a> JstfLangSys<'a> {
    /// Resolve the justification priorities in their table order.
    pub fn jstf_priorities(&self) -> ArrayOfOffsets<'a, JstfPriority<'a>, Offset16> {
        ArrayOfOffsets::new(self.jstf_priority_offsets(), self.offset_data(), ())
    }
}
