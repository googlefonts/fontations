//! the [GSUB] table
//!
//! [GSUB]: https://docs.microsoft.com/en-us/typography/opentype/spec/gsub

pub use super::layout::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, SanitizedCoverageTable, ScriptList, SequenceContext,
};
use super::layout::{ExtensionLookup, LookupFlag, Subtables};

#[cfg(feature = "std")]
mod closure;
#[cfg(test)]
#[path = "../tests/test_gsub.rs"]
mod tests;

include!("../../generated/generated_gsub.rs");

/// A typed GSUB [LookupList] table
pub type SubstitutionLookupList<'a> = LookupList<'a, SubstitutionLookup<'a>>;

pub type SanitizedSubstitutionLookupList<'a> = LookupList<'a, SanitizedSubstitutionLookup<'a>>;

/// A GSUB [SequenceContext]
pub type SubstitutionSequenceContext<'a> = super::layout::SequenceContext<'a>;

/// A GSUB [ChainedSequenceContext]
pub type SubstitutionChainContext<'a> = super::layout::ChainedSequenceContext<'a>;

impl<'a, T: FontReadWithArgs<'a, Args = ()>> ExtensionLookup<'a, T>
    for ExtensionSubstFormat1<'a, T>
{
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
                let first = offsets.first().ok_or(ReadError::OutOfBounds)?.get();
                let ext: ExtensionSubstFormat1<()> = first.resolve(data)?;
                match ext.extension_lookup_type() {
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

/// A [GSUB Lookup](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#gsubLookupTypeEnum) subtable.
pub enum SanitizedSubstitutionLookup<'a> {
    Single(Sanitized<Lookup<'a, SanitizedSingleSubst<'a>>>),
    Multiple(Sanitized<Lookup<'a, Sanitized<MultipleSubstFormat1<'a>>>>),
    Alternate(Sanitized<Lookup<'a, Sanitized<AlternateSubstFormat1<'a>>>>),
    Ligature(Sanitized<Lookup<'a, Sanitized<LigatureSubstFormat1<'a>>>>),
    Contextual(Sanitized<Lookup<'a, Sanitized<SubstitutionSequenceContext<'a>>>>),
    ChainContextual(Sanitized<Lookup<'a, Sanitized<SubstitutionChainContext<'a>>>>),
    Extension(Sanitized<Lookup<'a, Sanitized<ExtensionSubtable<'a>>>>),
    Reverse(Sanitized<Lookup<'a, Sanitized<ReverseChainSingleSubstFormat1<'a>>>>),
}

impl ReadArgs for SanitizedSubstitutionLookup<'_> {
    type Args = ();
}

impl<'a> FontReadWithArgs<'a> for SanitizedSubstitutionLookup<'a> {
    fn read_with_args(data: FontData<'a>, _: &Self::Args) -> Result<Self, ReadError> {
        unsafe { Ok(Self::read_with_args_unchecked(data, &())) }
    }
    unsafe fn read_with_args_unchecked(data: FontData<'a>, args: &Self::Args) -> Self {
        let untyped = Lookup::read_with_args_unchecked(data, args);
        match untyped.lookup_type() {
            1 => SanitizedSubstitutionLookup::Single(Sanitized(untyped.into_concrete())),
            2 => SanitizedSubstitutionLookup::Multiple(Sanitized(untyped.into_concrete())),
            3 => SanitizedSubstitutionLookup::Alternate(Sanitized(untyped.into_concrete())),
            4 => SanitizedSubstitutionLookup::Ligature(Sanitized(untyped.into_concrete())),
            5 => SanitizedSubstitutionLookup::Contextual(Sanitized(untyped.into_concrete())),
            6 => SanitizedSubstitutionLookup::ChainContextual(Sanitized(untyped.into_concrete())),
            7 => SanitizedSubstitutionLookup::Extension(Sanitized(untyped.into_concrete())),
            8 => SanitizedSubstitutionLookup::Reverse(Sanitized(untyped.into_concrete())),
            _ => unreachable!("sanitized"),
        }
    }
}

impl<'a> Sanitize<'a> for SanitizedSubstitutionLookup<'a> {
    fn sanitize_impl(&self) -> Result<(), ReadError> {
        Ok(())
    }
}

//impl<'a> Sanitized<Gsub<'a>> {
///// Attempt to resolve [`lookup_list_offset`][Self::lookup_list_offset].
//pub fn lookup_list(&self) -> Result<Sanitized<SanitizedSubstitutionLookupList<'a>>, ReadError> {
//let data = self.0.data;
//self.lookup_list_offset().resolve_with_args(data, &())
//}
//}
