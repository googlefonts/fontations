//! [OpenType™ Layout Common Table Formats](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2)

use font_types::{GlyphId, OffsetHost};

#[path = "../generated/generated_layout.rs"]
mod generated;

pub use generated::*;

#[cfg(feature = "compile")]
use crate::compile::{ToOwnedImpl, ToOwnedTable};

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
impl ToOwnedTable for ClassDef<'_> {}

#[cfg(feature = "compile")]
impl ToOwnedImpl for CoverageTable<'_> {
    type Owned = compile::CoverageTable;
    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        Some(self.iter().collect())
    }
}

#[cfg(feature = "compile")]
impl ToOwnedTable for CoverageTable<'_> {}

#[cfg(feature = "compile")]
pub mod compile {
    use font_types::GlyphId;
    use std::collections::BTreeMap;

    use crate::compile::Table;

    pub struct ClassDef {
        pub items: BTreeMap<GlyphId, u16>,
    }

    pub struct CoverageTable {
        // invariant: is always sorted
        glyphs: Vec<GlyphId>,
    }

    impl FromIterator<GlyphId> for CoverageTable {
        fn from_iter<T: IntoIterator<Item = GlyphId>>(iter: T) -> Self {
            let mut glyphs = iter.into_iter().collect::<Vec<_>>();
            glyphs.sort_unstable();
            CoverageTable { glyphs }
        }
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

    pub struct ClassDefFormat1Writer<'a> {
        inner: &'a ClassDef,
    }

    impl Table for ClassDefFormat1Writer<'_> {
        fn describe(&self, writer: &mut crate::compile::TableWriter) {
            writer.write(1u16);
            writer.write(*self.inner.items.keys().next().expect("no empty classdefs"));
            writer.write(self.inner.items.len() as u16);
            self.inner.items.values().for_each(|val| writer.write(*val));
        }
    }

    pub struct ClassDefFormat2Writer<'a> {
        inner: &'a ClassDef,
    }

    impl Table for ClassDefFormat2Writer<'_> {
        fn describe(&self, writer: &mut crate::compile::TableWriter) {
            writer.write(2u16);
            writer.write(iter_class_ranges(&self.inner.items).count() as u16);
            iter_class_ranges(&self.inner.items).for_each(|obj| {
                writer.write(obj.start_glyph_id);
                writer.write(obj.end_glyph_id);
                writer.write(obj.class);
            })
        }
    }

    impl Table for ClassDef {
        fn describe(&self, writer: &mut crate::compile::TableWriter) {
            let is_contiguous = self
                .items
                .keys()
                .zip(self.items.keys().skip(1))
                .all(|(a, b)| *b - *a == 1);
            if is_contiguous {
                ClassDefFormat1Writer { inner: self }.describe(writer)
            } else {
                ClassDefFormat2Writer { inner: self }.describe(writer)
            }
        }
    }

    fn iter_class_ranges(
        values: &BTreeMap<GlyphId, u16>,
    ) -> impl Iterator<Item = super::ClassRangeRecord> + '_ {
        let mut iter = values.iter();
        let mut prev = None;

        #[allow(clippy::while_let_on_iterator)]
        std::iter::from_fn(move || {
            while let Some((gid, class)) = iter.next() {
                match prev.take() {
                    None => prev = Some((*gid, *gid, *class)),
                    Some((start, end, pclass)) if (gid - end) == 1 && pclass == *class => {
                        prev = Some((start, *gid, pclass))
                    }
                    Some((start, end, pclass)) => {
                        prev = Some((*gid, *gid, *class));
                        return Some(super::ClassRangeRecord {
                            start_glyph_id: start.into(),
                            end_glyph_id: end.into(),
                            class: pclass.into(),
                        });
                    }
                }
            }
            prev.take()
                .map(|(start, end, class)| super::ClassRangeRecord {
                    start_glyph_id: start.into(),
                    end_glyph_id: end.into(),
                    class: class.into(),
                })
        })
    }

    impl Table for CoverageTable {
        fn describe(&self, writer: &mut crate::compile::TableWriter) {
            //TODO: use some heuristic to decide we should format2 sometimes
            writer.write(1u16);
            writer.write(self.glyphs.len() as u16);
            writer.write(self.glyphs.as_slice());
        }
    }
}
