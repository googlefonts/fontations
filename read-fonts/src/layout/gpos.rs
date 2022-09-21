//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

#[path = "./value_record.rs"]
mod value_record;

use crate::array::ComputedArray;

/// reexport stuff from layout that we use
pub use super::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, ScriptList, SequenceContext,
};
pub use value_record::ValueRecord;

#[cfg(test)]
#[path = "../tests/gpos.rs"]
mod tests;

/// 'GPOS'
pub const TAG: Tag = Tag::new(b"GPOS");

include!("../../generated/generated_gpos.rs");

pub type PositionLookupList<'a> = LookupList<'a, PositionLookup<'a>>;

/// A typed GPOS Lookup table
pub enum PositionLookup<'a> {
    Single(Lookup<'a, SinglePos<'a>>),
    Pair(Lookup<'a, PairPos<'a>>),
    Cursive(Lookup<'a, CursivePosFormat1<'a>>),
    MarkToBase(Lookup<'a, MarkBasePosFormat1<'a>>),
    MarkToMark(Lookup<'a, MarkMarkPosFormat1<'a>>),
    MarkToLig(Lookup<'a, MarkLigPosFormat1<'a>>),
    Contextual(Lookup<'a, SequenceContext<'a>>),
    ChainContextual(Lookup<'a, ChainedSequenceContext<'a>>),
    Extension(Lookup<'a, ExtensionSubtable<'a>>),
}

/// A typed extension subtable
//TODO: would be very nice to have codegen for this pattern...
pub enum ExtensionSubtable<'a> {
    Single(ExtensionPosFormat1<'a, SinglePos<'a>>),
    Pair(ExtensionPosFormat1<'a, PairPos<'a>>),
    Cursive(ExtensionPosFormat1<'a, CursivePosFormat1<'a>>),
    MarkToBase(ExtensionPosFormat1<'a, MarkBasePosFormat1<'a>>),
    MarkToLig(ExtensionPosFormat1<'a, MarkLigPosFormat1<'a>>),
    MarkToMark(ExtensionPosFormat1<'a, MarkMarkPosFormat1<'a>>),
    Contextual(ExtensionPosFormat1<'a, SequenceContext<'a>>),
    ChainContextual(ExtensionPosFormat1<'a, ChainedSequenceContext<'a>>),
}

impl<'a> FontRead<'a> for PositionLookup<'a> {
    fn read(bytes: FontData<'a>) -> Result<Self, ReadError> {
        let lookup = Lookup::read(bytes)?;
        match lookup.lookup_type() {
            1 => Ok(PositionLookup::Single(lookup.into_concrete())),
            2 => Ok(PositionLookup::Pair(lookup.into_concrete())),
            3 => Ok(PositionLookup::Cursive(lookup.into_concrete())),
            4 => Ok(PositionLookup::MarkToBase(lookup.into_concrete())),
            5 => Ok(PositionLookup::MarkToLig(lookup.into_concrete())),
            6 => Ok(PositionLookup::MarkToMark(lookup.into_concrete())),
            7 => Ok(PositionLookup::Contextual(lookup.into_concrete())),
            8 => Ok(PositionLookup::ChainContextual(lookup.into_concrete())),
            9 => Ok(PositionLookup::Extension(lookup.into_concrete())),
            other => Err(ReadError::InvalidFormat(other.into())),
        }
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for PositionLookup<'a> {
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match self {
            PositionLookup::Single(table) => table.get_field(idx),
            PositionLookup::Pair(table) => table.get_field(idx),
            PositionLookup::Cursive(table) => table.get_field(idx),
            PositionLookup::MarkToBase(table) => table.get_field(idx),
            PositionLookup::MarkToMark(table) => table.get_field(idx),
            PositionLookup::MarkToLig(table) => table.get_field(idx),
            PositionLookup::Contextual(table) => table.get_field(idx),
            PositionLookup::ChainContextual(table) => table.get_field(idx),
            PositionLookup::Extension(table) => table.get_field(idx),
        }
    }

    fn type_name(&self) -> &str {
        "Lookup"
    }
}

impl<'a> FontRead<'a> for ExtensionSubtable<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let extension = ExtensionPosFormat1::read(data)?;
        match extension.extension_lookup_type() {
            1 => Ok(ExtensionSubtable::Single(extension.into_concrete())),
            2 => Ok(ExtensionSubtable::Pair(extension.into_concrete())),
            3 => Ok(ExtensionSubtable::Cursive(extension.into_concrete())),
            4 => Ok(ExtensionSubtable::MarkToBase(extension.into_concrete())),
            5 => Ok(ExtensionSubtable::MarkToMark(extension.into_concrete())),
            6 => Ok(ExtensionSubtable::MarkToLig(extension.into_concrete())),
            7 => Ok(ExtensionSubtable::Contextual(extension.into_concrete())),
            8 => Ok(ExtensionSubtable::ChainContextual(
                extension.into_concrete(),
            )),
            other => Err(ReadError::InvalidFormat(other.into())),
        }
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for ExtensionSubtable<'a> {
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match self {
            ExtensionSubtable::Single(table) => table.get_field(idx),
            ExtensionSubtable::Pair(table) => table.get_field(idx),
            ExtensionSubtable::Cursive(table) => table.get_field(idx),
            ExtensionSubtable::MarkToBase(table) => table.get_field(idx),
            ExtensionSubtable::MarkToMark(table) => table.get_field(idx),
            ExtensionSubtable::MarkToLig(table) => table.get_field(idx),
            ExtensionSubtable::Contextual(table) => table.get_field(idx),
            ExtensionSubtable::ChainContextual(table) => table.get_field(idx),
        }
    }

    fn type_name(&self) -> &str {
        "ExtensionPosFormat1"
    }
}
