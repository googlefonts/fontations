//! [OpenType™ Layout Common Table Formats](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2)

use font_types::{GlyphId, OffsetHost};

#[path = "../generated/generated_layout.rs"]
mod generated;

pub use generated::*;

#[cfg(feature = "compile")]
use crate::compile::ToOwnedImpl;

impl<'a> LookupList<'a> {
    /// Iterate all of the [`Lookup`]s in this list.
    pub fn iter_lookups(&self) -> impl Iterator<Item = Lookup<'a>> + '_ {
        self.lookup_offsets()
            .iter()
            .filter_map(|off| self.resolve_offset(off.get()))
    }
}

impl CoverageTable<'_> {
    pub fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        // all one expression so that we have a single return type
        let (iter1, iter2) = match self {
            CoverageTable::Format1(t) => (Some(t.glyph_array().iter().map(|g| g.get())), None),
            CoverageTable::Format2(t) => {
                let iter = t
                    .range_records()
                    .iter()
                    .flat_map(|rcd| rcd.start_glyph_id()..=rcd.end_glyph_id());
                (None, Some(iter))
            }
        };

        iter1
            .into_iter()
            .flatten()
            .chain(iter2.into_iter().flatten())
    }
}

impl ClassDefFormat1<'_> {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        let start = self.start_glyph_id();
        self.class_value_array()
            .iter()
            .copied()
            .enumerate()
            .map(move |(i, cls)| (start + i as u16, cls.get()))
    }
}

impl ClassDefFormat2<'_> {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        self.class_range_records().iter().flat_map(|rcd| {
            (rcd.start_glyph_id()..=rcd.end_glyph_id()).map(|gid| (gid, rcd.class()))
        })
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for ClassDef<'_> {
    type Owned = compile::ClassDef;
    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let items = match self {
            ClassDef::Format1(t) => t.iter().collect(),
            ClassDef::Format2(t) => t.iter().collect(),
        };

        Some(compile::ClassDef { items })
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for CoverageTable<'_> {
    type Owned = compile::CoverageTable;
    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        Some(compile::CoverageTable {
            glyphs: self.iter().collect(),
        })
    }
}

#[cfg(feature = "std")]
pub mod compile {
    use font_types::GlyphId;
    use std::collections::BTreeMap;

    pub struct ClassDef {
        pub items: BTreeMap<GlyphId, u16>,
    }

    pub struct CoverageTable {
        // invariant: is always sorted
        pub glyphs: Vec<GlyphId>,
    }

    impl CoverageTable {
        /// Add a `GlyphId` to this coverage table.
        ///
        /// Returns the coverage index of the added glyph.
        ///
        /// If the glyph already exists, this returns its current index.
        pub fn add(&mut self, glyph: GlyphId) -> u16 {
            match self.glyphs.binary_search(&glyph) {
                Ok(ix) => ix as u16,
                Err(ix) => {
                    self.glyphs.insert(ix, glyph);
                    // if we're over u16::MAX glyphs, crash
                    ix.try_into().unwrap()
                }
            }
        }
    }
}
