//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

#[path = "./value_record.rs"]
mod value_record;

use crate::array::ComputedArray;

/// reexport stuff from layout that we use
pub use super::{
    ClassDef, CoverageTable, Device, FeatureList, FeatureVariations, Lookup, ScriptList,
};
pub use value_record::ValueRecord;

#[cfg(test)]
#[path = "../tests/gpos.rs"]
mod tests;

/// 'GPOS'
pub const TAG: Tag = Tag::new(b"GPOS");

include!("../../generated/generated_gpos.rs");

/// A typed GPOS [LookupList](super::LookupList) table
pub type PositionLookupList<'a> = super::LookupList<'a, PositionLookup<'a>>;

/// A GPOS [SequenceContext](super::SequenceContext)
pub type PositionSequenceContext<'a> = super::SequenceContext<'a>;

/// A GPOS [ChainedSequenceContext](super::ChainedSequenceContext)
pub type PositionChainContext<'a> = super::ChainedSequenceContext<'a>;
