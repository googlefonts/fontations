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

#[cfg(all(test, feature = "parsing"))]
#[path = "../tests/gpos.rs"]
mod tests;

/// A GPOS lookup list table.
#[derive(Debug, Clone)]
pub struct PositionLookupList {
    pub lookup_offsets: Vec<OffsetMarker<Offset16, PositionLookup>>,
}

impl Validate for PositionLookupList {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        ctx.in_field("lookup_offsets", |ctx| {
            self.lookup_offsets.validate_impl(ctx)
        });
    }
}

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
impl FromObjRef<font_tables::layout::gpos::PositionLookup<'_>> for PositionLookup {
    fn from_obj_ref(from: &font_tables::layout::gpos::PositionLookup<'_>, data: &FontData) -> Self {
        use font_tables::layout::gpos::PositionLookup as FromType;
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
impl FromTableRef<font_tables::layout::gpos::PositionLookup<'_>> for PositionLookup {}

#[cfg(feature = "parsing")]
impl FromObjRef<font_tables::layout::gpos::ExtensionSubtable<'_>> for Extension {
    fn from_obj_ref(
        from: &font_tables::layout::gpos::ExtensionSubtable<'_>,
        data: &FontData,
    ) -> Self {
        use font_tables::layout::gpos::ExtensionSubtable as FromType;
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
impl FromTableRef<font_tables::layout::gpos::ExtensionSubtable<'_>> for Extension {}

#[cfg(feature = "parsing")]
impl<'a> FontRead<'a> for PositionLookup {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        font_tables::layout::gpos::PositionLookup::read(data).map(|x| x.to_owned_table())
    }
}

#[cfg(feature = "parsing")]
impl<'a> FontRead<'a> for Extension {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        font_tables::layout::gpos::ExtensionSubtable::read(data).map(|x| x.to_owned_table())
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

#[cfg(feature = "parsing")]
impl FromObjRef<font_tables::layout::gpos::PositionLookupList<'_>> for PositionLookupList {
    fn from_obj_ref(
        from: &font_tables::layout::gpos::PositionLookupList<'_>,
        _data: &FontData,
    ) -> Self {
        PositionLookupList {
            lookup_offsets: from
                .lookups()
                .map(|lookup| OffsetMarker::new_maybe_null(lookup.ok().map(|x| x.to_owned_table())))
                .collect(),
        }
    }
}

#[cfg(feature = "parsing")]
impl<'a> FontRead<'a> for PositionLookupList {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        font_tables::layout::gpos::PositionLookupList::read(data).map(|x| x.to_owned_table())
    }
}

#[cfg(feature = "parsing")]
impl FromTableRef<font_tables::layout::gpos::PositionLookupList<'_>> for PositionLookupList {}

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
