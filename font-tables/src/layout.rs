//! [OpenType™ Layout Common Table Formats](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2)

use font_types::OffsetHost;

#[path = "../generated/generated_layout.rs"]
mod generated;

pub use generated::*;

impl<'a> LookupList<'a> {
    /// Iterate all of the [`Lookup`]s in this list.
    pub fn iter_lookups(&self) -> impl Iterator<Item = Lookup<'a>> + '_ {
        self.lookup_offsets()
            .iter()
            .filter_map(|off| self.resolve_offset(off.get()))
    }
}
