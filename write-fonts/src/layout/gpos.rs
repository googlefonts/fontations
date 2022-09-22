//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

include!("../../generated/generated_gpos.rs");

use std::collections::HashSet;

use super::value_record::ValueRecord;
use super::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, LookupType, ScriptList, SequenceContext,
};

#[cfg(all(test, feature = "parsing"))]
#[path = "../tests/gpos.rs"]
mod tests;

/// A GPOS lookup list table.
type PositionLookupList = LookupList<PositionLookup>;

impl Gpos {
    fn compute_version(&self) -> MajorMinor {
        if self.feature_variations_offset.get().is_none() {
            MajorMinor::VERSION_1_0
        } else {
            MajorMinor::VERSION_1_1
        }
    }
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
    Single(ExtensionPosFormat1<SinglePos>),
    Pair(ExtensionPosFormat1<PairPos>),
    Cursive(ExtensionPosFormat1<CursivePosFormat1>),
    MarkToBase(ExtensionPosFormat1<MarkBasePosFormat1>),
    MarkToLig(ExtensionPosFormat1<MarkLigPosFormat1>),
    MarkToMark(ExtensionPosFormat1<MarkMarkPosFormat1>),
    Contextual(ExtensionPosFormat1<SequenceContext>),
    ChainContextual(ExtensionPosFormat1<ChainedSequenceContext>),
}

impl<T: LookupType + FontWrite> FontWrite for ExtensionPosFormat1<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        1u16.write_into(writer);
        T::TYPE.write_into(writer);
        self.extension_offset.write_into(writer);
    }
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

impl Validate for PositionLookup {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        match self {
            Self::Single(lookup) => lookup.validate_impl(ctx),
            Self::Pair(lookup) => lookup.validate_impl(ctx),
            Self::Cursive(lookup) => lookup.validate_impl(ctx),
            Self::MarkToBase(lookup) => lookup.validate_impl(ctx),
            Self::MarkToLig(lookup) => lookup.validate_impl(ctx),
            Self::MarkToMark(lookup) => lookup.validate_impl(ctx),
            Self::Contextual(lookup) => lookup.validate_impl(ctx),
            Self::ChainContextual(lookup) => lookup.validate_impl(ctx),
            Self::Extension(lookup) => lookup.validate_impl(ctx),
        }
    }
}

#[cfg(feature = "parsing")]
impl FromObjRef<read_fonts::layout::gpos::PositionLookup<'_>> for PositionLookup {
    fn from_obj_ref(from: &read_fonts::layout::gpos::PositionLookup<'_>, data: FontData) -> Self {
        use read_fonts::layout::gpos::PositionLookup as FromType;
        match from {
            FromType::Single(lookup) => Self::Single(lookup.to_owned_obj(data)),
            FromType::Pair(lookup) => Self::Pair(lookup.to_owned_obj(data)),
            FromType::Cursive(lookup) => Self::Cursive(lookup.to_owned_obj(data)),
            FromType::MarkToBase(lookup) => Self::MarkToBase(lookup.to_owned_obj(data)),
            FromType::MarkToLig(lookup) => Self::MarkToLig(lookup.to_owned_obj(data)),
            FromType::MarkToMark(lookup) => Self::MarkToMark(lookup.to_owned_obj(data)),
            FromType::Contextual(lookup) => Self::Contextual(lookup.to_owned_obj(data)),
            FromType::ChainContextual(lookup) => Self::ChainContextual(lookup.to_owned_obj(data)),
            FromType::Extension(lookup) => Self::Extension(lookup.to_owned_obj(data)),
        }
    }
}

#[cfg(feature = "parsing")]
impl FromTableRef<read_fonts::layout::gpos::PositionLookup<'_>> for PositionLookup {}

#[cfg(feature = "parsing")]
impl FromObjRef<read_fonts::layout::gpos::ExtensionSubtable<'_>> for Extension {
    fn from_obj_ref(
        from: &read_fonts::layout::gpos::ExtensionSubtable<'_>,
        data: FontData,
    ) -> Self {
        use read_fonts::layout::gpos::ExtensionSubtable as FromType;
        match from {
            FromType::Single(ext) => Self::Single(ext.to_owned_obj(data)),
            FromType::Pair(ext) => Self::Pair(ext.to_owned_obj(data)),
            FromType::Cursive(ext) => Self::Cursive(ext.to_owned_obj(data)),
            FromType::MarkToBase(ext) => Self::MarkToBase(ext.to_owned_obj(data)),
            FromType::MarkToLig(ext) => Self::MarkToLig(ext.to_owned_obj(data)),
            FromType::MarkToMark(ext) => Self::MarkToMark(ext.to_owned_obj(data)),
            FromType::Contextual(ext) => Self::Contextual(ext.to_owned_obj(data)),
            FromType::ChainContextual(ext) => Self::ChainContextual(ext.to_owned_obj(data)),
        }
    }
}

#[cfg(feature = "parsing")]
impl FromTableRef<read_fonts::layout::gpos::ExtensionSubtable<'_>> for Extension {}

#[cfg(feature = "parsing")]
impl<'a> FontRead<'a> for PositionLookup {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::layout::gpos::PositionLookup::read(data).map(|x| x.to_owned_table())
    }
}

#[cfg(feature = "parsing")]
impl<'a> FontRead<'a> for Extension {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::layout::gpos::ExtensionSubtable::read(data).map(|x| x.to_owned_table())
    }
}

#[cfg(feature = "parsing")]
impl<'a> FontRead<'a> for PositionLookupList {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::layout::gpos::PositionLookupList::read(data).map(|x| x.to_owned_table())
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

impl Validate for Extension {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        match self {
            Self::Single(lookup) => lookup.validate_impl(ctx),
            Self::Pair(lookup) => lookup.validate_impl(ctx),
            Self::Cursive(lookup) => lookup.validate_impl(ctx),
            Self::MarkToBase(lookup) => lookup.validate_impl(ctx),
            Self::MarkToLig(lookup) => lookup.validate_impl(ctx),
            Self::MarkToMark(lookup) => lookup.validate_impl(ctx),
            Self::Contextual(lookup) => lookup.validate_impl(ctx),
            Self::ChainContextual(lookup) => lookup.validate_impl(ctx),
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
