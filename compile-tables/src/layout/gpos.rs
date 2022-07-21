//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

include!("../../generated/gpos.rs");

use std::collections::HashSet;

use super::value_record::ValueRecord;
use super::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, ExtensionSubtable, FeatureList,
    FeatureVariations, Lookup, ScriptList, SequenceContext,
};

/// A GPOS lookup list table.
#[derive(Debug, Clone)]
pub struct PositionLookupList {
    pub lookup_offsets: Vec<OffsetMarker<Offset16, PositionLookup>>,
}

/// A GPOS lookup
#[derive(Debug, Clone)]
pub enum PositionLookup {
    Single(Lookup<SinglePos>),
    Pair(Lookup<PairPos>),
    Cursive(Lookup<CursivePosFormat1>),
    MarkToBase(Lookup<MarkBasePosFormat1>),
    MarkToLig(Lookup<MarkLigPosFormat1>),
    MarkToMark(Lookup<MarkMarkPosFormat1>),
    Contextual(Lookup<SequenceContext>),
    ChainContextual(Lookup<ChainedSequenceContext>),
    Extension(Lookup<Extension>),
}

/// A GPOS extension subtable
#[derive(Debug, Clone)]
pub enum Extension {
    Single(ExtensionSubtable<SinglePos>),
    Pair(ExtensionSubtable<PairPos>),
    Cursive(ExtensionSubtable<CursivePosFormat1>),
    MarkToBase(ExtensionSubtable<MarkBasePosFormat1>),
    MarkToLig(ExtensionSubtable<MarkLigPosFormat1>),
    MarkToMark(ExtensionSubtable<MarkMarkPosFormat1>),
    Contextual(ExtensionSubtable<SequenceContext>),
    ChainContextual(ExtensionSubtable<ChainedSequenceContext>),
}

impl FontWrite for PositionLookup {
    fn write_into(&self, writer: &mut TableWriter) {
        match self {
            PositionLookup::Single(lookup) => lookup.write_into(writer),
            PositionLookup::Pair(lookup) => lookup.write_into(writer),
            PositionLookup::Cursive(lookup) => lookup.write_into(writer),
            PositionLookup::MarkToBase(lookup) => lookup.write_into(writer),
            PositionLookup::MarkToLig(lookup) => lookup.write_into(writer),
            PositionLookup::MarkToMark(lookup) => lookup.write_into(writer),
            PositionLookup::Contextual(lookup) => lookup.write_into(writer),
            PositionLookup::ChainContextual(lookup) => lookup.write_into(writer),
            PositionLookup::Extension(lookup) => lookup.write_into(writer),
        }
    }
}

impl FontWrite for PositionLookupList {
    fn write_into(&self, writer: &mut TableWriter) {
        u16::try_from(self.lookup_offsets.len())
            .unwrap()
            .write_into(writer);
        self.lookup_offsets.write_into(writer);
    }
}

impl FontWrite for Extension {
    fn write_into(&self, writer: &mut TableWriter) {
        match self {
            Self::Single(lookup) => lookup.write_into(writer),
            Self::Pair(lookup) => lookup.write_into(writer),
            Self::Cursive(lookup) => lookup.write_into(writer),
            Self::MarkToBase(lookup) => lookup.write_into(writer),
            Self::MarkToLig(lookup) => lookup.write_into(writer),
            Self::MarkToMark(lookup) => lookup.write_into(writer),
            Self::Contextual(lookup) => lookup.write_into(writer),
            Self::ChainContextual(lookup) => lookup.write_into(writer),
        }
    }
}

impl SinglePosFormat1 {
    fn compute_value_format(&self) -> ValueFormat {
        self.value_record.format()
    }
}

impl SinglePosFormat2 {
    fn compute_value_format(&self) -> ValueFormat {
        self.value_records
            .first()
            .map(ValueRecord::format)
            .unwrap_or(ValueFormat::empty())
    }
}

impl PairPosFormat1 {
    fn compute_value_format1(&self) -> ValueFormat {
        self.pair_set_offsets
            .first()
            .and_then(|off| off.get())
            .and_then(|pairset| pairset.pair_value_records.first())
            .map(|rec| rec.value_record1.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_value_format2(&self) -> ValueFormat {
        self.pair_set_offsets
            .first()
            .and_then(|off| off.get())
            .and_then(|pairset| pairset.pair_value_records.first())
            .map(|rec| rec.value_record2.format())
            .unwrap_or(ValueFormat::empty())
    }
}

impl PairPosFormat2 {
    fn compute_value_format1(&self) -> ValueFormat {
        self.class1_records
            .first()
            .and_then(|rec| rec.class2_records.first())
            .map(|rec| rec.value_record1.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_value_format2(&self) -> ValueFormat {
        self.class1_records
            .first()
            .and_then(|rec| rec.class2_records.first())
            .map(|rec| rec.value_record2.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_class1_count(&self) -> u16 {
        self.class_def1_offset
            .get()
            .map(|cls| cls.class_count())
            .unwrap_or_default()
    }

    fn compute_class2_count(&self) -> u16 {
        self.class_def2_offset
            .get()
            .map(|cls| cls.class_count())
            .unwrap_or_default()
    }
}

impl MarkBasePosFormat1 {
    fn compute_mark_class_count(&self) -> u16 {
        mark_array_class_count(self.mark_array_offset.get())
    }
}

impl MarkMarkPosFormat1 {
    fn compute_mark_class_count(&self) -> u16 {
        mark_array_class_count(self.mark1_array_offset.get())
    }
}

impl MarkLigPosFormat1 {
    fn compute_mark_class_count(&self) -> u16 {
        mark_array_class_count(self.mark_array_offset.get())
    }
}

fn mark_array_class_count(mark_array: Option<&MarkArray>) -> u16 {
    mark_array.map(MarkArray::class_count).unwrap_or_default()
}

impl MarkArray {
    fn class_count(&self) -> u16 {
        self.mark_records
            .iter()
            .map(|rec| rec.mark_class)
            .collect::<HashSet<_>>()
            .len() as u16
    }
}
