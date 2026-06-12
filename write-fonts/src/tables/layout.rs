//! OpenType layout.

use std::{
    collections::{BTreeMap, HashSet},
    hash::Hash,
};

pub use read_fonts::tables::layout::LookupFlag;
use read_fonts::FontRead;

pub mod builders;
#[cfg(test)]
mod spec_tests;

include!("../../generated/generated_layout.rs");

/// A macro to implement the [LookupSubtable] trait.
macro_rules! lookup_type {
    (gpos, $ty:ty, $val:expr) => {
        impl LookupSubtable for $ty {
            const TYPE: LookupType = LookupType::Gpos($val);
        }
    };

    (gsub, $ty:ty, $val:expr) => {
        impl LookupSubtable for $ty {
            const TYPE: LookupType = LookupType::Gsub($val);
        }
    };
}

/// A macro to define a newtype around an existing table, that defers all
/// impls to that table.
///
/// We use this to ensure that shared lookup types (Sequence/Chain
/// lookups) can be given different lookup ids for each of GSUB/GPOS.
macro_rules! table_newtype {
    ($name:ident, $inner:ident, $read_type:path) => {
        /// A typed wrapper around a shared table.
        ///
        /// This is used so that we can associate the correct lookup ids for
        /// lookups that are shared between GPOS/GSUB.
        ///
        /// You can access the inner type via `Deref` or the `as_inner` method.
        #[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name($inner);

        impl $name {
            /// Return a reference to the inner type.
            pub fn as_inner(&self) -> &$inner {
                &self.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = $inner;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl FontWrite for $name {
            fn write_into(&self, writer: &mut TableWriter) {
                self.0.write_into(writer)
            }

            fn table_type(&self) -> crate::table_type::TableType {
                self.0.table_type()
            }
        }

        impl Validate for $name {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                self.0.validate_impl(ctx)
            }
        }

        impl<'a> FromObjRef<$read_type> for $name {
            fn from_obj_ref(obj: &$read_type, _data: FontData) -> Self {
                Self(FromObjRef::from_obj_ref(obj, _data))
            }
        }

        impl<'a> FromTableRef<$read_type> for $name {}

        impl From<$inner> for $name {
            fn from(src: $inner) -> $name {
                $name(src)
            }
        }
    };
}

pub(crate) use lookup_type;
pub(crate) use table_newtype;

impl FontWrite for LookupFlag {
    fn write_into(&self, writer: &mut TableWriter) {
        self.to_bits().write_into(writer)
    }
}

impl<T: LookupSubtable + FontWrite> FontWrite for Lookup<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        T::TYPE.write_into(writer);
        self.lookup_flag.write_into(writer);
        u16::try_from(self.subtables.len())
            .unwrap()
            .write_into(writer);
        self.subtables.write_into(writer);
        self.mark_filtering_set.write_into(writer);
    }

    fn table_type(&self) -> crate::table_type::TableType {
        T::TYPE.into()
    }
}

impl Lookup<SequenceContext> {
    /// Convert this untyped SequenceContext into its GSUB or GPOS specific version
    pub fn into_concrete<T: From<SequenceContext>>(self) -> Lookup<T> {
        let Lookup {
            lookup_flag,
            subtables,
            mark_filtering_set,
        } = self;
        let subtables = subtables
            .into_iter()
            .map(|offset| OffsetMarker::new(offset.into_inner().into()))
            .collect();
        Lookup {
            lookup_flag,
            subtables,
            mark_filtering_set,
        }
    }
}

impl Lookup<ChainedSequenceContext> {
    /// Convert this untyped SequenceContext into its GSUB or GPOS specific version
    pub fn into_concrete<T: From<ChainedSequenceContext>>(self) -> Lookup<T> {
        let Lookup {
            lookup_flag,
            subtables,
            mark_filtering_set,
        } = self;
        let subtables = subtables
            .into_iter()
            .map(|offset| OffsetMarker::new(offset.into_inner().into()))
            .collect();
        Lookup {
            lookup_flag,
            subtables,
            mark_filtering_set,
        }
    }
}

/// A utility trait for writing lookup tables.
///
/// This allows us to attach the numerical lookup type to the appropriate concrete
/// types, so that we can write it as needed without passing it around.
pub trait LookupSubtable {
    /// The lookup type of this layout subtable.
    const TYPE: LookupType;
}

/// Raw values for the different layout subtables
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupType {
    Gpos(u16),
    Gsub(u16),
}

impl LookupType {
    pub(crate) const GSUB_EXT_TYPE: u16 = 7;
    pub(crate) const GPOS_EXT_TYPE: u16 = 9;
    pub(crate) const PAIR_POS: u16 = 2;
    pub(crate) const MARK_TO_BASE: u16 = 4;

    pub(crate) fn to_raw(self) -> u16 {
        match self {
            LookupType::Gpos(val) => val,
            LookupType::Gsub(val) => val,
        }
    }

    pub(crate) fn promote(self) -> Self {
        match self {
            LookupType::Gpos(Self::GPOS_EXT_TYPE) | LookupType::Gsub(Self::GSUB_EXT_TYPE) => {
                panic!("should never be promoting an extension subtable")
            }
            LookupType::Gpos(_) => LookupType::Gpos(Self::GPOS_EXT_TYPE),
            LookupType::Gsub(_) => LookupType::Gsub(Self::GSUB_EXT_TYPE),
        }
    }
}

impl FontWrite for LookupType {
    fn write_into(&self, writer: &mut TableWriter) {
        self.to_raw().write_into(writer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

impl Validate for FeatureParams {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        match self {
            Self::StylisticSet(table) => table.validate_impl(ctx),
            Self::Size(table) => table.validate_impl(ctx),
            Self::CharacterVariant(table) => table.validate_impl(ctx),
        }
    }
}

impl FromObjRef<read_fonts::tables::layout::FeatureParams<'_>> for FeatureParams {
    fn from_obj_ref(from: &read_fonts::tables::layout::FeatureParams, data: FontData) -> Self {
        use read_fonts::tables::layout::FeatureParams as FromType;
        match from {
            FromType::Size(thing) => Self::Size(SizeParams::from_obj_ref(thing, data)),
            FromType::StylisticSet(thing) => {
                Self::StylisticSet(FromObjRef::from_obj_ref(thing, data))
            }
            FromType::CharacterVariant(thing) => {
                Self::CharacterVariant(FromObjRef::from_obj_ref(thing, data))
            }
        }
    }
}

impl FromTableRef<read_fonts::tables::layout::FeatureParams<'_>> for FeatureParams {}

impl ClassDefFormat1 {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        self.class_value_array.iter().enumerate().map(|(i, cls)| {
            (
                GlyphId::from(GlyphId16::new(
                    self.start_glyph_id.to_u16().saturating_add(i as u16),
                )),
                *cls,
            )
        })
    }
}

impl ClassRangeRecord {
    fn validate_glyph_range(&self, ctx: &mut ValidationCtx) {
        if self.start_glyph_id > self.end_glyph_id {
            ctx.report(format!(
                "start_glyph_id {} larger than end_glyph_id {}",
                self.start_glyph_id, self.end_glyph_id
            ));
        }
    }

    fn contains(&self, gid: GlyphId16) -> bool {
        (self.start_glyph_id..=self.end_glyph_id).contains(&gid)
    }
}

impl ClassDefFormat2 {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        self.class_range_records.iter().flat_map(|rcd| {
            (rcd.start_glyph_id.to_u16()..=rcd.end_glyph_id.to_u16())
                .map(|gid| (GlyphId::from(GlyphId16::new(gid)), rcd.class))
        })
    }
}

impl ClassDefFormat3 {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        let start = self.start_glyph_id.to_u32();
        self.class_value_array
            .iter()
            .enumerate()
            .map(move |(i, cls)| (GlyphId::new(start + i as u32), *cls))
    }
}

impl ClassRangeRecord2 {
    fn validate_glyph_range(&self, ctx: &mut ValidationCtx) {
        if self.start_glyph_id > self.end_glyph_id {
            ctx.report(format!(
                "start_glyph_id {} larger than end_glyph_id {}",
                self.start_glyph_id, self.end_glyph_id
            ));
        }
    }

    fn contains(&self, gid: GlyphId) -> bool {
        (self.start_glyph_id.to_u32()..=self.end_glyph_id.to_u32()).contains(&gid.to_u32())
    }
}

impl ClassDefFormat4 {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        self.class_range_records.iter().flat_map(|rcd| {
            (rcd.start_glyph_id.to_u32()..=rcd.end_glyph_id.to_u32())
                .map(|gid| (GlyphId::new(gid), rcd.class))
        })
    }
}

impl ClassDef {
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        let (one, two, three, four) = match self {
            Self::Format1(table) => (Some(table.iter()), None, None, None),
            Self::Format2(table) => (None, Some(table.iter()), None, None),
            Self::Format3(table) => (None, None, Some(table.iter()), None),
            Self::Format4(table) => (None, None, None, Some(table.iter())),
        };

        one.into_iter()
            .flatten()
            .chain(two.into_iter().flatten())
            .chain(three.into_iter().flatten())
            .chain(four.into_iter().flatten())
    }

    /// Return the glyph class for the provided glyph.
    ///
    /// Glyphs which have not been assigned a class are given class 0
    pub fn get(&self, glyph: impl Into<GlyphId>) -> u16 {
        self.get_raw(glyph).unwrap_or(0)
    }

    // exposed for testing
    fn get_raw(&self, glyph: impl Into<GlyphId>) -> Option<u16> {
        let glyph = glyph.into();
        match self {
            ClassDef::Format1(table) => {
                let glyph = GlyphId16::try_from(glyph).ok()?;
                glyph
                    .to_u16()
                    .checked_sub(table.start_glyph_id.to_u16())
                    .and_then(|idx| table.class_value_array.get(idx as usize))
                    .copied()
            }
            ClassDef::Format2(table) => table.class_range_records.iter().find_map(|rec| {
                rec.contains(GlyphId16::try_from(glyph).ok()?)
                    .then_some(rec.class)
            }),
            ClassDef::Format3(table) => glyph
                .to_u32()
                .checked_sub(table.start_glyph_id.to_u32())
                .and_then(|idx| table.class_value_array.get(idx as usize))
                .copied(),
            ClassDef::Format4(table) => table
                .class_range_records
                .iter()
                .find_map(|rec| rec.contains(glyph).then_some(rec.class)),
        }
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

    /// Returns `true` if no glyphs are explicitly assigned to a class in this table
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Format1(table) => table.class_value_array.is_empty(),
            Self::Format2(table) => table.class_range_records.is_empty(),
            Self::Format3(table) => table.class_value_array.is_empty(),
            Self::Format4(table) => table.class_range_records.is_empty(),
        }
    }
}

impl CoverageFormat1 {
    fn iter(&self) -> impl Iterator<Item = GlyphId16> + '_ {
        self.glyph_array.iter().copied()
    }

    fn len(&self) -> usize {
        self.glyph_array.len()
    }
}

impl CoverageFormat2 {
    fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        self.range_records
            .iter()
            .flat_map(|rcd| iter_gids(rcd.start_glyph_id, rcd.end_glyph_id))
            .map(GlyphId::from)
    }

    fn len(&self) -> usize {
        self.range_records
            .iter()
            .map(|rcd| {
                rcd.end_glyph_id
                    .to_u16()
                    .saturating_sub(rcd.start_glyph_id.to_u16()) as usize
                    + 1
            })
            .sum()
    }
}

impl CoverageTable {
    pub fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        let one = match self {
            Self::Format1(table) => Some(table.iter().map(GlyphId::from)),
            _ => None,
        };
        let two = match self {
            Self::Format2(table) => Some(table.iter()),
            _ => None,
        };
        let three = match self {
            Self::Format3(table) => Some(table.iter()),
            _ => None,
        };
        let four = match self {
            Self::Format4(table) => Some(table.iter()),
            _ => None,
        };

        one.into_iter()
            .flatten()
            .chain(two.into_iter().flatten())
            .chain(three.into_iter().flatten())
            .chain(four.into_iter().flatten())
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Format1(table) => table.len(),
            Self::Format2(table) => table.len(),
            Self::Format3(table) => table.len(),
            Self::Format4(table) => table.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl CoverageFormat3 {
    fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        self.glyph_array.iter().copied().map(GlyphId::from)
    }

    fn len(&self) -> usize {
        self.glyph_array.len()
    }
}

impl CoverageFormat4 {
    fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        self.range_records
            .iter()
            .flat_map(|rcd| iter_gids24(rcd.start_glyph_id, rcd.end_glyph_id).map(GlyphId::from))
    }

    fn len(&self) -> usize {
        self.range_records
            .iter()
            .map(|rcd| {
                rcd.end_glyph_id
                    .to_u32()
                    .saturating_sub(rcd.start_glyph_id.to_u32()) as usize
                    + 1
            })
            .sum()
    }
}

impl FromIterator<GlyphId16> for CoverageTable {
    fn from_iter<T: IntoIterator<Item = GlyphId16>>(iter: T) -> Self {
        iter.into_iter().map(GlyphId::from).collect()
    }
}

impl FromIterator<GlyphId> for CoverageTable {
    fn from_iter<T: IntoIterator<Item = GlyphId>>(iter: T) -> Self {
        let mut glyphs = iter.into_iter().collect::<Vec<_>>();
        glyphs.sort();
        glyphs.dedup();

        if let Some(glyphs16) = glyphs
            .iter()
            .copied()
            .map(|gid| GlyphId16::try_from(gid).ok())
            .collect::<Option<Vec<_>>>()
        {
            builders::CoverageTableBuilder::from_glyphs(glyphs16).build()
        } else {
            let glyphs = glyphs
                .into_iter()
                .map(|gid| GlyphId24::try_from(gid).expect("glyph id exceeds 24 bits"))
                .collect::<Vec<_>>();
            if should_choose_coverage_format_4(&glyphs) {
                CoverageTable::Format4(CoverageFormat4 {
                    range_records: RangeRecord2::iter_for_glyphs(&glyphs).collect(),
                })
            } else {
                CoverageTable::Format3(CoverageFormat3 {
                    glyph_array: glyphs,
                })
            }
        }
    }
}

impl From<Vec<GlyphId16>> for CoverageTable {
    fn from(value: Vec<GlyphId16>) -> Self {
        builders::CoverageTableBuilder::from_glyphs(value).build()
    }
}

impl FromIterator<(GlyphId16, u16)> for ClassDef {
    fn from_iter<T: IntoIterator<Item = (GlyphId16, u16)>>(iter: T) -> Self {
        builders::ClassDefBuilderImpl::from_iter(iter).build()
    }
}

impl FromIterator<(GlyphId, u16)> for ClassDef {
    fn from_iter<T: IntoIterator<Item = (GlyphId, u16)>>(iter: T) -> Self {
        let items = iter
            .into_iter()
            .filter(|(_, cls)| *cls != 0)
            .collect::<BTreeMap<_, _>>();

        if let Some(items16) = items
            .iter()
            .map(|(gid, cls)| GlyphId16::try_from(*gid).ok().map(|gid| (gid, *cls)))
            .collect::<Option<Vec<_>>>()
        {
            builders::ClassDefBuilderImpl::from_iter(items16).build()
        } else if should_choose_classdef_format_3(&items) {
            let first = items.keys().next().copied().unwrap_or_default().to_u32();
            let last = items
                .keys()
                .next_back()
                .copied()
                .unwrap_or_default()
                .to_u32();
            let class_value_array = (first..=last)
                .map(|gid| items.get(&GlyphId::new(gid)).copied().unwrap_or(0))
                .collect();
            ClassDef::Format3(ClassDefFormat3 {
                start_glyph_id: GlyphId24::new(first),
                class_value_array,
            })
        } else {
            ClassDef::Format4(ClassDefFormat4 {
                class_range_records: iter_class_ranges24(&items).collect(),
            })
        }
    }
}

impl RangeRecord {
    /// An iterator over records for this array of glyphs.
    ///
    /// # Note
    ///
    /// this function expects that glyphs are already sorted.
    pub fn iter_for_glyphs(glyphs: &[GlyphId16]) -> impl Iterator<Item = RangeRecord> + '_ {
        let mut cur_range = glyphs.first().copied().map(|g| (g, g));
        let mut len = 0u16;
        let mut iter = glyphs.iter().skip(1).copied();

        #[allow(clippy::while_let_on_iterator)]
        std::iter::from_fn(move || {
            while let Some(glyph) = iter.next() {
                match cur_range {
                    None => return None,
                    Some((a, b)) if are_sequential(b, glyph) => cur_range = Some((a, glyph)),
                    Some((a, b)) => {
                        let result = RangeRecord {
                            start_glyph_id: a,
                            end_glyph_id: b,
                            start_coverage_index: len,
                        };
                        cur_range = Some((glyph, glyph));
                        len += 1 + b.to_u16().saturating_sub(a.to_u16());
                        return Some(result);
                    }
                }
            }
            cur_range
                .take()
                .map(|(start_glyph_id, end_glyph_id)| RangeRecord {
                    start_glyph_id,
                    end_glyph_id,
                    start_coverage_index: len,
                })
        })
    }
}

impl RangeRecord2 {
    /// An iterator over records for this array of glyphs.
    ///
    /// # Note
    ///
    /// this function expects that glyphs are already sorted.
    pub fn iter_for_glyphs(glyphs: &[GlyphId24]) -> impl Iterator<Item = RangeRecord2> + '_ {
        let mut cur_range = glyphs.first().copied().map(|g| (g, g));
        let mut len = 0u32;
        let mut iter = glyphs.iter().skip(1).copied();

        #[allow(clippy::while_let_on_iterator)]
        std::iter::from_fn(move || {
            while let Some(glyph) = iter.next() {
                match cur_range {
                    None => return None,
                    Some((a, b)) if are_sequential24(b, glyph) => cur_range = Some((a, glyph)),
                    Some((a, b)) => {
                        let result = RangeRecord2 {
                            start_glyph_id: a,
                            end_glyph_id: b,
                            start_coverage_index: Uint24::new(len),
                        };
                        cur_range = Some((glyph, glyph));
                        len += 1 + b.to_u32().saturating_sub(a.to_u32());
                        return Some(result);
                    }
                }
            }
            cur_range
                .take()
                .map(|(start_glyph_id, end_glyph_id)| RangeRecord2 {
                    start_glyph_id,
                    end_glyph_id,
                    start_coverage_index: Uint24::new(len),
                })
        })
    }
}

fn iter_gids(gid1: GlyphId16, gid2: GlyphId16) -> impl Iterator<Item = GlyphId16> {
    (gid1.to_u16()..=gid2.to_u16()).map(GlyphId16::new)
}

fn iter_gids24(gid1: GlyphId24, gid2: GlyphId24) -> impl Iterator<Item = GlyphId24> {
    (gid1.to_u32()..=gid2.to_u32()).map(GlyphId24::new)
}

fn are_sequential(gid1: GlyphId16, gid2: GlyphId16) -> bool {
    gid2.to_u16().saturating_sub(gid1.to_u16()) == 1
}

fn are_sequential24(gid1: GlyphId24, gid2: GlyphId24) -> bool {
    gid2.to_u32().saturating_sub(gid1.to_u32()) == 1
}

fn should_choose_coverage_format_4(glyphs: &[GlyphId24]) -> bool {
    let format4_len = 5 + RangeRecord2::iter_for_glyphs(glyphs).count() * 9;
    let format3_len = 5 + glyphs.len() * 3;
    format4_len < format3_len
}

fn should_choose_classdef_format_3(values: &BTreeMap<GlyphId, u16>) -> bool {
    let Some(first) = values.keys().next() else {
        return false;
    };
    let last = values.keys().next_back().unwrap();
    let span_len = last.to_u32() - first.to_u32() + 1;
    if span_len > Uint24::MAX.to_u32() {
        return false;
    }

    let format3_len = 8 + span_len as usize * 2;
    let format4_len = 5 + iter_class_ranges24(values).count() * 8;
    format3_len < format4_len
}

fn iter_class_ranges24(
    values: &BTreeMap<GlyphId, u16>,
) -> impl Iterator<Item = ClassRangeRecord2> + '_ {
    let mut iter = values.iter();
    let mut prev = None;

    #[allow(clippy::while_let_on_iterator)]
    std::iter::from_fn(move || {
        while let Some((gid, class)) = iter.next() {
            match prev.take() {
                None => prev = Some((*gid, *gid, *class)),
                Some((start, end, pclass))
                    if gid.to_u32().saturating_sub(end.to_u32()) == 1 && pclass == *class =>
                {
                    prev = Some((start, *gid, pclass))
                }
                Some((start_glyph_id, end_glyph_id, pclass)) => {
                    prev = Some((*gid, *gid, *class));
                    return Some(ClassRangeRecord2 {
                        start_glyph_id: GlyphId24::try_from(start_glyph_id)
                            .expect("glyph id exceeds 24 bits"),
                        end_glyph_id: GlyphId24::try_from(end_glyph_id)
                            .expect("glyph id exceeds 24 bits"),
                        class: pclass,
                    });
                }
            }
        }
        prev.take()
            .map(|(start_glyph_id, end_glyph_id, class)| ClassRangeRecord2 {
                start_glyph_id: GlyphId24::try_from(start_glyph_id)
                    .expect("glyph id exceeds 24 bits"),
                end_glyph_id: GlyphId24::try_from(end_glyph_id).expect("glyph id exceeds 24 bits"),
                class,
            })
    })
}

impl Device {
    pub fn new(start_size: u16, end_size: u16, values: &[i8]) -> Self {
        debug_assert_eq!(
            (start_size..=end_size).count(),
            values.len(),
            "device range and values must match"
        );
        let delta_format: DeltaFormat = values
            .iter()
            .map(|val| match val {
                -2..=1 => DeltaFormat::Local2BitDeltas,
                -8..=7 => DeltaFormat::Local4BitDeltas,
                _ => DeltaFormat::Local8BitDeltas,
            })
            .max()
            .unwrap_or_default();
        let delta_value = encode_delta(delta_format, values);

        Device {
            start_size,
            end_size,
            delta_format,
            delta_value,
        }
    }
}

impl DeviceOrVariationIndex {
    /// Create a new [`Device`] subtable
    pub fn device(start_size: u16, end_size: u16, values: &[i8]) -> Self {
        DeviceOrVariationIndex::Device(Device::new(start_size, end_size, values))
    }
}

impl FontWrite for PendingVariationIndex {
    fn write_into(&self, _writer: &mut TableWriter) {
        panic!(
            "Attempted to write PendingVariationIndex.\n\
            VariationIndex tables should always be resolved before compilation.\n\
            Please report this bug at <https://github.com/googlefonts/fontations/issues>"
        )
    }
}

fn encode_delta(format: DeltaFormat, values: &[i8]) -> Vec<u16> {
    let (chunk_size, mask, bits) = match format {
        DeltaFormat::Local2BitDeltas => (8, 0b11, 2),
        DeltaFormat::Local4BitDeltas => (4, 0b1111, 4),
        DeltaFormat::Local8BitDeltas => (2, 0b11111111, 8),
        _ => panic!("invalid format"),
    };
    values
        .chunks(chunk_size)
        .map(|chunk| encode_chunk(chunk, mask, bits))
        .collect()
}

fn encode_chunk(chunk: &[i8], mask: u8, bits: usize) -> u16 {
    let mut out = 0u16;
    for (i, val) in chunk.iter().enumerate() {
        out |= ((val.to_be_bytes()[0] & mask) as u16) << ((16 - bits) - i * bits);
    }
    out
}

impl From<VariationIndex> for u32 {
    fn from(value: VariationIndex) -> Self {
        ((value.delta_set_outer_index as u32) << 16) | value.delta_set_inner_index as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gids(values: &[u32]) -> Vec<GlyphId> {
        values.iter().copied().map(GlyphId::new).collect()
    }

    #[test]
    fn coverage_collects_high_sparse_glyphs_as_format4() {
        let coverage = gids(&[
            0x1_0000, 0x1_0001, 0x1_0002, 0x1_0003, 0x2_0000, 0x2_0001, 0x2_0002, 0x2_0003,
        ])
        .into_iter()
        .collect::<CoverageTable>();

        let CoverageTable::Format4(coverage) = coverage else {
            panic!("expected CoverageFormat4");
        };
        assert_eq!(coverage.range_records.len(), 2);
        assert_eq!(
            coverage.range_records[0].start_glyph_id,
            GlyphId24::new(0x1_0000)
        );
        assert_eq!(
            coverage.range_records[0].end_glyph_id,
            GlyphId24::new(0x1_0003)
        );
        assert_eq!(
            coverage.range_records[0].start_coverage_index,
            Uint24::new(0)
        );
        assert_eq!(
            coverage.range_records[1].start_glyph_id,
            GlyphId24::new(0x2_0000)
        );
        assert_eq!(
            coverage.range_records[1].end_glyph_id,
            GlyphId24::new(0x2_0003)
        );
        assert_eq!(
            coverage.range_records[1].start_coverage_index,
            Uint24::new(4)
        );
    }

    #[test]
    fn coverage_collects_high_dense_glyphs_as_format3() {
        let coverage = gids(&[0x1_0000, 0x1_0100, 0x1_0200, 0x1_0300])
            .into_iter()
            .collect::<CoverageTable>();

        let CoverageTable::Format3(coverage) = coverage else {
            panic!("expected CoverageFormat3");
        };
        assert_eq!(
            coverage.glyph_array,
            vec![
                GlyphId24::new(0x1_0000),
                GlyphId24::new(0x1_0100),
                GlyphId24::new(0x1_0200),
                GlyphId24::new(0x1_0300),
            ]
        );
    }

    #[test]
    fn classdef_collects_high_dense_glyphs_as_format3() {
        let classdef = [
            (GlyphId::new(0x1_0000), 5),
            (GlyphId::new(0x1_0001), 0),
            (GlyphId::new(0x1_0002), 7),
        ]
        .into_iter()
        .collect::<ClassDef>();

        let ClassDef::Format3(classdef) = classdef else {
            panic!("expected ClassDefFormat3");
        };
        assert_eq!(classdef.start_glyph_id, GlyphId24::new(0x1_0000));
        assert_eq!(classdef.class_value_array, vec![5, 0, 7]);
    }

    #[test]
    fn classdef_collects_high_sparse_glyphs_as_format4() {
        let classdef = [
            (GlyphId::new(0x1_0000), 4),
            (GlyphId::new(0x1_0001), 4),
            (GlyphId::new(0x1_0002), 4),
            (GlyphId::new(0x2_0000), 7),
            (GlyphId::new(0x2_0001), 7),
        ]
        .into_iter()
        .collect::<ClassDef>();

        let ClassDef::Format4(classdef) = classdef else {
            panic!("expected ClassDefFormat4");
        };
        assert_eq!(classdef.class_range_records.len(), 2);
        assert_eq!(
            classdef.class_range_records[0],
            ClassRangeRecord2::new(GlyphId24::new(0x1_0000), GlyphId24::new(0x1_0002), 4)
        );
        assert_eq!(
            classdef.class_range_records[1],
            ClassRangeRecord2::new(GlyphId24::new(0x2_0000), GlyphId24::new(0x2_0001), 7)
        );
    }

    #[test]
    #[should_panic(expected = "array exceeds max length")]
    fn array_len_smoke_test() {
        let table = ScriptList {
            script_records: vec![ScriptRecord {
                script_tag: Tag::new(b"hihi"),
                script: OffsetMarker::new(Script {
                    default_lang_sys: NullableOffsetMarker::new(None),
                    lang_sys_records: vec![LangSysRecord {
                        lang_sys_tag: Tag::new(b"coco"),
                        lang_sys: OffsetMarker::new(LangSys {
                            required_feature_index: 0xffff,
                            feature_indices: vec![69; (u16::MAX) as usize + 5],
                        }),
                    }],
                }),
            }],
        };

        table.validate().unwrap();
    }

    #[test]
    #[should_panic(expected = "larger than end_glyph_id")]
    fn validate_classdef_ranges() {
        let classdef = ClassDefFormat2::new(vec![ClassRangeRecord::new(
            GlyphId16::new(12),
            GlyphId16::new(3),
            7,
        )]);

        classdef.validate().unwrap();
    }

    #[test]
    fn delta_encode() {
        let inp = [1i8, 2, 3, -1];
        let result = encode_delta(DeltaFormat::Local4BitDeltas, &inp);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0x123f_u16);

        let inp = [1i8, 1, 1, 1, 1];
        let result = encode_delta(DeltaFormat::Local2BitDeltas, &inp);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0x5540_u16);
    }
}
