//! The [JSTF](https://learn.microsoft.com/en-us/typography/opentype/spec/jstf) table.

pub use super::gpos::PositionLookupList;

include!("../../generated/generated_jstf.rs");

impl<'a> JstfLangSys<'a> {
    /// Resolve the justification priorities in their table order.
    pub fn jstf_priorities(&self) -> ArrayOfOffsets<'a, JstfPriority<'a>, Offset16> {
        ArrayOfOffsets::new(self.jstf_priority_offsets(), self.offset_data(), ())
    }
}
