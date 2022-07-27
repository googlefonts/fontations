//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

#[path = "./valuerecord.rs"]
mod valuerecord;

use crate::array::ComputedArray;

use super::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, ScriptList, SequenceContext, TypedLookup,
};
pub use valuerecord::ValueRecord;

include!("../../generated/gpos.rs");

/// A typed GPOS LookupList table
pub struct PositionLookupList<'a>(LookupList<'a>);

/// A typed GPOS Lookup table
pub enum PositionLookup<'a> {
    Single(TypedLookup<'a, SinglePos<'a>>),
    Pair(TypedLookup<'a, PairPos<'a>>),
    Cursive(TypedLookup<'a, CursivePosFormat1<'a>>),
    MarkToBase(TypedLookup<'a, MarkBasePosFormat1<'a>>),
    MarkToMark(TypedLookup<'a, MarkMarkPosFormat1<'a>>),
    MarkToLig(TypedLookup<'a, MarkLigPosFormat1<'a>>),
    Contextual(TypedLookup<'a, SequenceContext<'a>>),
    ChainContextual(TypedLookup<'a, ChainedSequenceContext<'a>>),
    Extension(TypedLookup<'a, ExtensionPosFormat1<'a>>),
}

impl<'a> PositionLookupList<'a> {
    pub fn lookup_count(&self) -> u16 {
        self.0.lookup_count()
    }

    pub fn iter(&self) -> impl Iterator<Item = PositionLookup<'a>> + '_ {
        self.0
            .lookup_offsets()
            .iter()
            .flat_map(|off| self.0.resolve_offset(off.get()))
    }
}

impl<'a> FontRead<'a> for PositionLookup<'a> {
    fn read(bytes: FontData<'a>) -> Result<Self, ReadError> {
        let lookup = Lookup::read(bytes)?;
        match lookup.lookup_type() {
            1 => Ok(PositionLookup::Single(TypedLookup::new(lookup))),
            2 => Ok(PositionLookup::Pair(TypedLookup::new(lookup))),
            3 => Ok(PositionLookup::Cursive(TypedLookup::new(lookup))),
            4 => Ok(PositionLookup::MarkToBase(TypedLookup::new(lookup))),
            5 => Ok(PositionLookup::MarkToLig(TypedLookup::new(lookup))),
            6 => Ok(PositionLookup::MarkToMark(TypedLookup::new(lookup))),
            7 => Ok(PositionLookup::Contextual(TypedLookup::new(lookup))),
            8 => Ok(PositionLookup::ChainContextual(TypedLookup::new(lookup))),
            9 => Ok(PositionLookup::Extension(TypedLookup::new(lookup))),
            other => Err(ReadError::InvalidFormat(other)),
        }
    }
}

impl<'a> std::ops::Deref for PositionLookup<'a> {
    type Target = Lookup<'a>;
    fn deref(&self) -> &Self::Target {
        match self {
            PositionLookup::Single(table) => *&table,
            PositionLookup::Pair(table) => *&table,
            PositionLookup::Cursive(table) => *&table,
            PositionLookup::MarkToBase(table) => *&table,
            PositionLookup::MarkToMark(table) => *&table,
            PositionLookup::MarkToLig(table) => *&table,
            PositionLookup::Contextual(table) => *&table,
            PositionLookup::ChainContextual(table) => *&table,
            PositionLookup::Extension(table) => *&table,
        }
    }
}

impl<'a> FontRead<'a> for PositionLookupList<'a> {
    fn read(bytes: FontData<'a>) -> Result<Self, ReadError> {
        LookupList::read(bytes).map(Self)
    }
}

fn class1_record_len(
    class1_count: u16,
    class2_count: u16,
    format1: ValueFormat,
    format2: ValueFormat,
) -> usize {
    (format1.record_byte_len() + format2.record_byte_len())
        * class1_count as usize
        * class2_count as usize
}

impl<'a> SinglePosFormat1<'a> {
    pub fn value_record(&self) -> ValueRecord {
        self.data
            .read_at_with_args(
                self.shape.value_record_byte_range().start,
                &self.value_format(),
            )
            .unwrap_or_default()
    }
}

impl<'a> SinglePosFormat2<'a> {
    pub fn value_records(&self) -> ComputedArray<'a, ValueFormat, ValueRecord> {
        ComputedArray::new(
            self.data
                .slice(self.shape.value_records_byte_range())
                .unwrap_or_default(),
            self.value_format(),
        )
    }
}
