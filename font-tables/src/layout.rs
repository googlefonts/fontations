//! [OpenType™ Layout Common Table Formats](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2)

#[cfg(feature = "compile")]
use crate::compile::{NullableOffsetMarker, OffsetMarker, ToOwnedObj};
use font_types::{GlyphId, OffsetHost};

include!("../generated/generated_layout_parse.rs");

/// A typed lookup table.
///
/// Our generated code doesn't handle generics, so we define this ourselves.
pub struct TypedLookup<'a, T> {
    inner: Lookup<'a>,
    phantom: std::marker::PhantomData<T>,
}

// this is all made complicated by our need to pass through the feature tag
// in order to correctly deserialize the FeatureParams field
impl FeatureList<'_> {
    #[cfg(feature = "compile")]
    pub(crate) fn feature_records_to_owned(&self) -> Option<Vec<compile::FeatureRecord>> {
        Some(
            self.feature_records()
                .iter()
                .flat_map(|rec| rec.to_owned_obj_custom(self.bytes()))
                .collect(),
        )
    }
}

#[cfg(feature = "compile")]
impl FeatureRecord {
    fn to_owned_obj_custom(&self, data: &[u8]) -> Option<compile::FeatureRecord> {
        let feature = self.feature_offset().read::<Feature>(data)?;
        Some(compile::FeatureRecord {
            feature_tag: self.feature_tag(),
            feature_offset: OffsetMarker::new_maybe_null(
                feature.to_owned_obj_custom(self.feature_tag()),
            ),
        })
    }
}

#[cfg(feature = "compile")]
impl Feature<'_> {
    fn to_owned_obj_custom(&self, tag: Tag) -> Option<compile::Feature> {
        Some(compile::Feature {
            feature_params_offset: NullableOffsetMarker::new(
                self.feature_params_offset()
                    .read_with_args::<_, FeatureParams>(self.bytes(), &tag)
                    .and_then(|obj| obj.to_owned_obj(self.bytes())),
            ),
            lookup_list_indices: self.lookup_list_indices().iter().map(|g| g.get()).collect(),
        })
    }
}

#[cfg(feature = "compile")]
impl FeatureTableSubstitution<'_> {
    fn substitutions_to_owned(&self) -> Option<Vec<compile::FeatureTableSubstitutionRecord>> {
        Some(
            self.substitutions()
                .iter()
                .flat_map(|rec| {
                    let feature = rec
                        .alternate_feature_offset()
                        .read::<Feature>(self.bytes())?;
                    Some(compile::FeatureTableSubstitutionRecord {
                        feature_index: rec.feature_index(),
                        alternate_feature_offset: OffsetMarker::new_maybe_null(
                            feature.to_owned_obj_custom(Tag::new(b"LOVE")),
                        ),
                    })
                })
                .collect(),
        )
    }
}

pub enum FeatureParams<'a> {
    StylisticSet(StylisticSetParams),
    Size(SizeParams),
    CharacterVariant(CharacterVariantParams<'a>),
}

impl<'a> FontReadWithArgs<'a, Tag> for FeatureParams<'a> {
    fn read_with_args(bytes: &'a [u8], args: &Tag) -> Option<(Self, &'a [u8])> {
        match *args {
            t if t == Tag::new(b"size") => {
                let r = SizeParams::read(bytes).map(Self::Size)?;
                Some((r, bytes.get(std::mem::size_of::<SizeParams>()..)?))
            }
            // to whoever is debugging this dumb bug I wrote: I'm sorry.
            t if &t.to_raw()[..2] == b"ss" => {
                let r = StylisticSetParams::read(bytes).map(Self::StylisticSet)?;
                Some((r, bytes.get(std::mem::size_of::<StylisticSetParams>()..)?))
            }
            t if &t.to_raw()[..2] == b"cv" => CharacterVariantParams::read(bytes)
                .map(Self::CharacterVariant)
                .map(|r| (r, &bytes[..0])),
            _ => None,
        }
    }
}

impl<'a, T: FontRead<'a>> TypedLookup<'a, T> {
    pub(crate) fn new(inner: Lookup<'a>) -> Self {
        TypedLookup {
            inner,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn subtables<'b: 'a>(&'b self) -> impl Iterator<Item = T> + 'a {
        self.inner
            .subtable_offsets()
            .iter()
            .flat_map(|off| self.inner.resolve_offset(off.get()))
    }
}

impl<'a, T> std::ops::Deref for TypedLookup<'a, T> {
    type Target = Lookup<'a>;
    fn deref(&self) -> &Self::Target {
        &self.inner
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

impl ClassDef<'_> {
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        let (one, two) = match self {
            Self::Format1(table) => (Some(table.iter()), None),
            Self::Format2(table) => (None, Some(table.iter())),
        };

        one.into_iter().flatten().chain(two.into_iter().flatten())
    }
}

#[cfg(feature = "compile")]
pub mod compile {
    use font_types::{FontRead, GlyphId, Offset, Offset16, OffsetHost};
    use std::collections::{BTreeMap, HashSet};

    use crate::compile::{FontWrite, OffsetMarker, ToOwnedTable};

    include!("../generated/generated_layout_compile.rs");

    fn plus_one(inp: usize) -> u16 {
        inp.saturating_add(1).try_into().unwrap()
    }

    pub trait LayoutSubtable {
        const TYPE: u16;
    }

    macro_rules! subtable_type {
        ($ty:ty, $val:expr) => {
            impl LayoutSubtable for $ty {
                const TYPE: u16 = $val;
            }
        };
    }

    subtable_type!(crate::tables::gpos::compile::SinglePos, 1);
    subtable_type!(crate::tables::gpos::compile::PairPos, 2);
    subtable_type!(crate::tables::gpos::compile::CursivePosFormat1, 3);
    subtable_type!(crate::tables::gpos::compile::MarkBasePosFormat1, 4);
    subtable_type!(crate::tables::gpos::compile::MarkLigPosFormat1, 5);
    subtable_type!(crate::tables::gpos::compile::MarkMarkPosFormat1, 6);
    subtable_type!(SequenceContext, 7);
    subtable_type!(ChainedSequenceContext, 8);
    subtable_type!(crate::tables::gpos::compile::Extension, 9);

    #[derive(Debug, PartialEq)]
    pub struct Lookup<T> {
        pub lookup_flag: u16,
        pub subtables: Vec<OffsetMarker<Offset16, T>>,
        pub mark_filtering_set: u16,
    }

    impl<T: LayoutSubtable + FontWrite> FontWrite for Lookup<T> {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            T::TYPE.write_into(writer);
            self.lookup_flag.write_into(writer);
            u16::try_from(self.subtables.len())
                .unwrap()
                .write_into(writer);
            self.subtables.write_into(writer);
            self.mark_filtering_set.write_into(writer);
        }
    }

    impl<'a, T: FontRead<'a> + ToOwnedTable> ToOwnedObj for super::TypedLookup<'a, T> {
        type Owned = Lookup<T::Owned>;

        fn to_owned_obj(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
            let subtables: Vec<OffsetMarker<Offset16, T::Owned>> = self
                .subtable_offsets()
                .iter()
                .map(|off| {
                    off.get()
                        .read::<T>(self.bytes())
                        .and_then(|t| t.to_owned_table().map(OffsetMarker::new))
                })
                .collect::<Option<_>>()?;
            Some(Lookup {
                lookup_flag: self.lookup_flag(),
                subtables,
                mark_filtering_set: self.mark_filtering_set(),
            })
        }
    }

    #[derive(Debug, PartialEq)]
    pub enum FeatureParams {
        StylisticSet(StylisticSetParams),
        Size(SizeParams),
        CharacterVariant(CharacterVariantParams),
    }

    impl FontWrite for FeatureParams {
        fn write_into(&self, writer: &mut TableWriter) {
            match self {
                FeatureParams::StylisticSet(table) => table.write_into(writer),
                FeatureParams::Size(table) => table.write_into(writer),
                FeatureParams::CharacterVariant(table) => table.write_into(writer),
            }
        }
    }

    impl ToOwnedObj for super::FeatureParams<'_> {
        type Owned = FeatureParams;

        fn to_owned_obj(&self, offset_data: &[u8]) -> Option<Self::Owned> {
            match self {
                Self::StylisticSet(table) => table
                    .to_owned_obj(offset_data)
                    .map(FeatureParams::StylisticSet),
                Self::Size(table) => table.to_owned_obj(offset_data).map(FeatureParams::Size),
                Self::CharacterVariant(table) => table
                    .to_owned_obj(offset_data)
                    .map(FeatureParams::CharacterVariant),
            }
        }
    }

    impl<'a, T: FontRead<'a> + ToOwnedTable> ToOwnedTable for super::TypedLookup<'a, T> {}

    #[derive(Debug, PartialEq)]
    pub struct ClassDefBuilder {
        pub items: BTreeMap<GlyphId, u16>,
    }

    // represent the format choice as a type. this would let us provide API
    // such that the user could manually decide which format to use, if wished
    pub struct ClassDefFormat1Writer<'a>(&'a ClassDefBuilder);

    pub struct ClassDefFormat2Writer<'a>(&'a ClassDefBuilder);

    #[derive(Debug, PartialEq)]
    pub struct CoverageTableBuilder {
        // invariant: is always sorted
        glyphs: Vec<GlyphId>,
    }

    pub struct CoverageTableFormat1Writer<'a>(&'a CoverageTableBuilder);

    pub struct CoverageTableFormat2Writer<'a>(&'a CoverageTableBuilder);

    impl FromIterator<GlyphId> for CoverageTableBuilder {
        fn from_iter<T: IntoIterator<Item = GlyphId>>(iter: T) -> Self {
            let mut glyphs = iter.into_iter().collect::<Vec<_>>();
            glyphs.sort_unstable();
            CoverageTableBuilder { glyphs }
        }
    }

    impl CoverageTableBuilder {
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

    impl ClassDefFormat1 {
        fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
            self.class_value_array
                .iter()
                .enumerate()
                .map(|(i, cls)| (self.start_glyph_id.saturating_add(i as u16), *cls))
        }
    }

    impl ClassDefFormat2 {
        fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
            self.class_range_records
                .iter()
                .flat_map(|rcd| (rcd.start_glyph_id..=rcd.end_glyph_id).map(|gid| (gid, rcd.class)))
        }
    }

    impl ClassDef {
        pub fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
            let (one, two) = match self {
                Self::Format1(table) => (Some(table.iter()), None),
                Self::Format2(table) => (None, Some(table.iter())),
            };

            one.into_iter().flatten().chain(two.into_iter().flatten())
        }

        pub fn class_count(&self) -> u16 {
            //TODO: implement a good integer set!!
            self.iter()
                .map(|(_gid, cls)| cls)
                .chain(std::iter::once(0))
                .collect::<HashSet<_>>()
                .len()
                .try_into()
                .unwrap()
        }
    }

    impl FromIterator<(GlyphId, u16)> for ClassDefBuilder {
        fn from_iter<T: IntoIterator<Item = (GlyphId, u16)>>(iter: T) -> Self {
            Self {
                items: iter.into_iter().collect(),
            }
        }
    }

    impl FontWrite for ClassDefFormat1Writer<'_> {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            1u16.write_into(writer);
            (*self.0.items.keys().next().expect("no empty classdefs")).write_into(writer);
            (self.0.items.len() as u16).write_into(writer);
            self.0.items.values().for_each(|val| val.write_into(writer));
        }
    }

    impl FontWrite for ClassDefFormat2Writer<'_> {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            2u16.write_into(writer);
            (iter_class_ranges(&self.0.items).count() as u16).write_into(writer);
            iter_class_ranges(&self.0.items).for_each(|obj| {
                obj.start_glyph_id.write_into(writer);
                obj.end_glyph_id.write_into(writer);
                obj.class.write_into(writer);
            })
        }
    }

    impl FontWrite for ClassDefBuilder {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            let is_contiguous = self
                .items
                .keys()
                .zip(self.items.keys().skip(1))
                .all(|(a, b)| *b - *a == 1);
            if is_contiguous {
                ClassDefFormat1Writer(self).write_into(writer)
            } else {
                ClassDefFormat2Writer(self).write_into(writer)
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

    impl FontWrite for CoverageTableBuilder {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            if should_choose_coverage_format_2(&self.glyphs) {
                CoverageTableFormat2Writer(self).write_into(writer);
            } else {
                CoverageTableFormat1Writer(self).write_into(writer);
            }
        }
    }

    impl FontWrite for CoverageTableFormat1Writer<'_> {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            1u16.write_into(writer);
            (self.0.glyphs.len() as u16).write_into(writer);
            self.0.glyphs.as_slice().write_into(writer);
        }
    }

    impl FontWrite for CoverageTableFormat2Writer<'_> {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            2u16.write_into(writer);
            (iter_ranges(&self.0.glyphs).count() as u16).write_into(writer);
            for range in iter_ranges(&self.0.glyphs) {
                range.start_glyph_id.write_into(writer);
                range.end_glyph_id.write_into(writer);
                range.start_coverage_index.write_into(writer);
            }
        }
    }

    //TODO: this can be fancier; we probably want to do something like find the
    // percentage of glyphs that are in contiguous ranges, or something?
    fn should_choose_coverage_format_2(glyphs: &[GlyphId]) -> bool {
        glyphs.len() > 3
            && glyphs
                .iter()
                .zip(glyphs.iter().skip(1))
                .all(|(a, b)| b - a == 1)
    }

    fn iter_ranges(glyphs: &[GlyphId]) -> impl Iterator<Item = super::RangeRecord> + '_ {
        let mut cur_range = glyphs.first().copied().map(|g| (g, g));
        let mut len = 0u16;
        let mut iter = glyphs.iter().skip(1).copied();

        #[allow(clippy::while_let_on_iterator)]
        std::iter::from_fn(move || {
            while let Some(glyph) = iter.next() {
                match cur_range {
                    None => return None,
                    Some((a, b)) if glyph - b == 1 => cur_range = Some((a, glyph)),
                    Some((a, b)) => {
                        let result = super::RangeRecord {
                            start_glyph_id: a.into(),
                            end_glyph_id: b.into(),
                            start_coverage_index: len.into(),
                        };
                        cur_range = Some((glyph, glyph));
                        len += 1 + b - a;
                        return Some(result);
                    }
                }
            }
            cur_range.take().map(|(start, end)| super::RangeRecord {
                start_glyph_id: start.into(),
                end_glyph_id: end.into(),
                start_coverage_index: len.into(),
            })
        })
    }
}

#[cfg(feature = "compile")]
#[cfg(test)]
mod compile_tests {
    use crate::assert_hex_eq;
    use crate::compile::ToOwnedObj;
    use font_types::OffsetHost;

    use super::*;

    #[test]
    fn example_1_scripts() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-1-scriptlist-table-and-scriptrecords
        #[rustfmt::skip]
        let bytes = [
            0x00, 0x03, 0x68, 0x61, 0x6E, 0x69, 0x00, 0x14, 0x6B, 0x61, 0x6E,
            0x61, 0x00, 0x18, 0x6C, 0x61, 0x74, 0x6E, 0x00, 0x1C,
        ];

        let table = ScriptList::read(&bytes).unwrap();
        assert_eq!(table.script_count(), 3);
        let first = table.script_records()[0];
        assert_eq!(first.script_tag(), Tag::new(b"hani"));
        assert_eq!(first.script_offset(), 0x14);
        //NOTE: we can't roundtrip this because the data doesn't include subtables.
        //assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn example_2_scripts_and_langs() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-2-script-table-langsysrecord-and-langsys-table
        #[rustfmt::skip]
        let bytes = [
            0x00, 0x0A, 0x00, 0x01, 0x55, 0x52, 0x44, 0x20, 0x00, 0x16, 0x00,
            0x00, 0xFF, 0xFF, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02,
            0x00, 0x00, 0x00, 0x03, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x02,
        ];

        let table = Script::read(&bytes).unwrap();
        let owned = table.to_owned_obj(&[]).unwrap();
        let dumped = crate::compile::dump_table(&owned);
        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn example_3_featurelist_and_feature() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-3-featurelist-table-and-feature-table
        #[rustfmt::skip]
        let bytes = [
            0x00, 0x03, 0x6C, 0x69, 0x67, 0x61, 0x00, 0x14, 0x6C, 0x69, 0x67,
            0x61, 0x00, 0x1A, 0x6C, 0x69, 0x67, 0x61, 0x00, 0x22, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02,
        ];

        let table = FeatureList::read(&bytes).unwrap();
        assert_eq!(table.feature_count(), 3);
        let turkish_liga = table.feature_records()[0]
            .feature_offset()
            .read::<Feature>(table.bytes())
            .unwrap();
        assert_eq!(turkish_liga.lookup_index_count(), 1);
        let owned = table.to_owned_obj(&[]).unwrap();
        let dumped = crate::compile::dump_table(&owned);
        assert_hex_eq!(&bytes, &dumped);
    }
}
