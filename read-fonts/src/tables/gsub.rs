//! the [GSUB] table
//!
//! [GSUB]: https://docs.microsoft.com/en-us/typography/opentype/spec/gsub

pub use super::layout::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, ScriptList, SequenceContext,
};
use super::layout::{ExtensionLookup, LookupFlag, Subtables};

#[cfg(feature = "std")]
mod closure;
#[cfg(test)]
#[path = "../tests/test_gsub.rs"]
mod tests;

include!("../../generated/generated_gsub.rs");

impl<'a> Gsub<'a> {
    /// Resolves the table, preferring the 32-bit offset when present.
    pub fn script_list(&self) -> Result<ScriptList<'a>, ReadError> {
        super::layout::extended::preferred_offset(
            self.script_list_offset(),
            self.script_list2_offset(),
            self.version() >= MajorMinor::new(1, 2),
        )?
        .resolve(self.offset_data())
    }

    /// Resolves the original 16-bit offset without applying precedence.
    pub fn legacy_script_list(&self) -> Option<Result<ScriptList<'a>, ReadError>> {
        self.script_list_offset().resolve(self.offset_data())
    }

    /// Resolves the table, preferring the 32-bit offset when present.
    pub fn feature_list(&self) -> Result<FeatureList<'a>, ReadError> {
        super::layout::extended::preferred_offset(
            self.feature_list_offset(),
            self.feature_list2_offset(),
            self.version() >= MajorMinor::new(1, 2),
        )?
        .resolve(self.offset_data())
    }

    /// Resolves the original 16-bit offset without applying precedence.
    pub fn legacy_feature_list(&self) -> Option<Result<FeatureList<'a>, ReadError>> {
        self.feature_list_offset().resolve(self.offset_data())
    }

    /// Resolves the lookup list, preferring LookupList2 when its offset is nonzero.
    pub fn lookup_list(&self) -> Result<SubstitutionLookupList<'a>, ReadError> {
        let offset = super::layout::extended::preferred_offset(
            self.lookup_list_offset(),
            self.lookup_list2_offset(),
            self.version() >= MajorMinor::new(1, 2),
        )?;
        if self.version() >= MajorMinor::new(1, 2)
            && self
                .lookup_list2_offset()
                .is_some_and(|offset| !offset.offset().is_null())
        {
            offset
                .resolve(self.offset_data())
                .map(super::layout::LookupListTable::Offset32)
        } else {
            offset
                .resolve(self.offset_data())
                .map(super::layout::LookupListTable::Offset16)
        }
    }

    /// Resolves the original 16-bit lookup-list offset.
    pub fn legacy_lookup_list(
        &self,
    ) -> Option<Result<LegacySubstitutionLookupList<'a>, ReadError>> {
        self.lookup_list_offset().resolve(self.offset_data())
    }
}

/// A typed GSUB [LookupList] table
pub type SubstitutionLookupList<'a> = super::layout::LookupListTable<'a, SubstitutionLookup<'a>>;

/// A lookup list with the original 16-bit offsets.
pub type LegacySubstitutionLookupList<'a> = super::layout::LookupList<'a, SubstitutionLookup<'a>>;

/// A lookup list with 32-bit offsets.
pub type SubstitutionLookupList2<'a> = super::layout::LookupList2<'a, SubstitutionLookup<'a>>;

/// A GSUB [SequenceContext]
pub type SubstitutionSequenceContext<'a> = super::layout::SequenceContext<'a>;

/// A GSUB [ChainedSequenceContext]
pub type SubstitutionChainContext<'a> = super::layout::ChainedSequenceContext<'a>;

impl<'a, T: FontRead<'a, Args = ()>> ExtensionLookup<'a, T> for ExtensionSubstFormat1<'a, T> {
    fn extension(&self) -> Result<T, ReadError> {
        self.extension()
    }
}

type SubSubtables<'a, T> = Subtables<'a, T, ExtensionSubstFormat1<'a, T>>;

/// The subtables from a GPOS lookup.
///
/// This type is a convenience that removes the need to dig into the
/// [`SubstitutionLookup`] enum in order to access subtables, and it also abstracts
/// away the distinction between extension and non-extension lookups.
pub enum SubstitutionSubtables<'a> {
    Single(SubSubtables<'a, SingleSubst<'a>>),
    Multiple(SubSubtables<'a, MultipleSubstFormat1<'a>>),
    Alternate(SubSubtables<'a, AlternateSubstFormat1<'a>>),
    Ligature(SubSubtables<'a, LigatureSubstFormat1<'a>>),
    Contextual(SubSubtables<'a, SubstitutionSequenceContext<'a>>),
    ChainContextual(SubSubtables<'a, SubstitutionChainContext<'a>>),
    Reverse(SubSubtables<'a, ReverseChainSingleSubstFormat1<'a>>),
    /// An extension lookup did not have any subtables
    EmptyExtension,
}

impl<'a> SubstitutionLookup<'a> {
    pub fn lookup_flag(&self) -> LookupFlag {
        self.of_unit_type().lookup_flag()
    }

    /// Different enumerations for GSUB and GPOS
    pub fn lookup_type(&self) -> u16 {
        self.of_unit_type().lookup_type()
    }

    pub fn mark_filtering_set(&self) -> Option<u16> {
        self.of_unit_type().mark_filtering_set()
    }

    /// Return the subtables for this lookup.
    ///
    /// This method handles both extension and non-extension lookups, and saves
    /// the caller needing to dig into the `SubstitutionLookup` enum itself.
    pub fn subtables(&self) -> Result<SubstitutionSubtables<'a>, ReadError> {
        let raw_lookup = self.of_unit_type();
        let offsets = raw_lookup.subtable_offsets();
        let data = raw_lookup.offset_data();
        match raw_lookup.lookup_type() {
            1 => Ok(SubstitutionSubtables::Single(Subtables::new(offsets, data))),
            2 => Ok(SubstitutionSubtables::Multiple(Subtables::new(
                offsets, data,
            ))),
            3 => Ok(SubstitutionSubtables::Alternate(Subtables::new(
                offsets, data,
            ))),
            4 => Ok(SubstitutionSubtables::Ligature(Subtables::new(
                offsets, data,
            ))),
            5 => Ok(SubstitutionSubtables::Contextual(Subtables::new(
                offsets, data,
            ))),
            6 => Ok(SubstitutionSubtables::ChainContextual(Subtables::new(
                offsets, data,
            ))),
            8 => Ok(SubstitutionSubtables::Reverse(Subtables::new(
                offsets, data,
            ))),
            7 => {
                // look through subtable offsets to try and find a lookup type.
                // this is robust in the case where the first subtable offset is
                // malformed, but a later one is okay.
                let Some(lookup_type) = offsets.iter().find_map(|off| {
                    off.get()
                        .resolve::<ExtensionSubstFormat1<()>>(data)
                        .ok()
                        .map(|ext| ext.extension_lookup_type())
                }) else {
                    return Ok(SubstitutionSubtables::EmptyExtension);
                };

                match lookup_type {
                    1 => Ok(SubstitutionSubtables::Single(Subtables::new_ext(
                        offsets, data,
                    ))),
                    2 => Ok(SubstitutionSubtables::Multiple(Subtables::new_ext(
                        offsets, data,
                    ))),
                    3 => Ok(SubstitutionSubtables::Alternate(Subtables::new_ext(
                        offsets, data,
                    ))),
                    4 => Ok(SubstitutionSubtables::Ligature(Subtables::new_ext(
                        offsets, data,
                    ))),
                    5 => Ok(SubstitutionSubtables::Contextual(Subtables::new_ext(
                        offsets, data,
                    ))),
                    6 => Ok(SubstitutionSubtables::ChainContextual(Subtables::new_ext(
                        offsets, data,
                    ))),
                    8 => Ok(SubstitutionSubtables::Reverse(Subtables::new_ext(
                        offsets, data,
                    ))),
                    other => Err(ReadError::InvalidFormat(other as _)),
                }
            }
            other => Err(ReadError::InvalidFormat(other as _)),
        }
    }
}
