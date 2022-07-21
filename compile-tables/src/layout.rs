//! OpenType layout

use std::collections::HashSet;

pub mod gpos;
mod value_record;

include!("../generated/layout.rs");

/// A lookup table that is generic over the lookup type.
#[derive(Debug, Clone)]
pub struct Lookup<T> {
    pub lookup_flag: u16,
    pub subtables: Vec<OffsetMarker<Offset16, T>>,
    pub mark_filtering_set: u16,
}

impl<T: LookupType + FontWrite> FontWrite for Lookup<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        T::TYPE.write_into(writer);
        self.lookup_flag.write_into(writer);
        u16::try_from(self.subtables.len())
            .unwrap()
            .write_into(writer);
        self.subtables.write_into(writer);
        self.mark_filtering_set.write_into(writer);
    }
}

/// An extension table that is generic over the subtable type.
#[derive(Debug, Clone)]
pub struct ExtensionSubtable<T> {
    pub extension_offset: OffsetMarker<Offset32, T>,
}

impl<T: LookupType + FontWrite> FontWrite for ExtensionSubtable<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        1u16.write_into(writer);
        T::TYPE.write_into(writer);
        self.extension_offset.write_into(writer);
    }
}

/// A utility trait for writing lookup tables
pub trait LookupType {
    /// The format type of this subtable.
    const TYPE: u16;
}

macro_rules! subtable_type {
    ($ty:ty, $val:expr) => {
        impl LookupType for $ty {
            const TYPE: u16 = $val;
        }
    };
}

subtable_type!(gpos::SinglePos, 1);
subtable_type!(gpos::PairPos, 2);
subtable_type!(gpos::CursivePosFormat1, 3);
subtable_type!(gpos::MarkBasePosFormat1, 4);
subtable_type!(gpos::MarkLigPosFormat1, 5);
subtable_type!(gpos::MarkMarkPosFormat1, 6);
subtable_type!(SequenceContext, 7);
subtable_type!(ChainedSequenceContext, 8);
subtable_type!(gpos::Extension, 9);

#[derive(Debug, Clone)]
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
