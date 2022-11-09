//! the [GSUB] table
//!
//! [GSUB]: https://docs.microsoft.com/en-us/typography/opentype/spec/gsub

include!("../../generated/generated_gsub.rs");

use super::{
    ChainedSequenceContext, CoverageTable, FeatureList, FeatureVariations, Lookup, LookupList,
    LookupType, ScriptList, SequenceContext,
};

#[cfg(test)]
#[path = "../tests/test_gsub.rs"]
mod tests;

/// A GSUB lookup list table.
type SubstitutionLookupList = LookupList<SubstitutionLookup>;

table_newtype!(
    SubstitutionSequenceContext,
    SequenceContext,
    read_fonts::layout::SequenceContext<'a>
);

table_newtype!(
    SubstitutionChainContext,
    ChainedSequenceContext,
    read_fonts::layout::ChainedSequenceContext<'a>
);

impl Gsub {
    fn compute_version(&self) -> MajorMinor {
        if self.feature_variations.is_none() {
            MajorMinor::VERSION_1_0
        } else {
            MajorMinor::VERSION_1_1
        }
    }
}

lookup_type!(SingleSubst, 1);
lookup_type!(MultipleSubstFormat1, 2);
lookup_type!(AlternateSubstFormat1, 3);
lookup_type!(LigatureSubstFormat1, 4);
lookup_type!(SubstitutionSequenceContext, 5);
lookup_type!(SubstitutionChainContext, 6);
lookup_type!(ExtensionSubtable, 7);
lookup_type!(ReverseChainSingleSubstFormat1, 8);

impl<T: LookupType + FontWrite> FontWrite for ExtensionSubstFormat1<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        1u16.write_into(writer);
        T::TYPE.write_into(writer);
        self.extension.write_into(writer);
    }
}

// these can't have auto impls because the traits don't support generics
impl<'a> FontRead<'a> for SubstitutionLookup {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::layout::gsub::SubstitutionLookup::read(data).map(|x| x.to_owned_table())
    }
}

impl<'a> FontRead<'a> for SubstitutionLookupList {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::layout::gsub::SubstitutionLookupList::read(data).map(|x| x.to_owned_table())
    }
}
