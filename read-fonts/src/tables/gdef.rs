//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

pub use super::layout::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, DeviceOrVariationIndex, FeatureList,
    FeatureVariations, Lookup, LookupList, ScriptList, SequenceContext,
};

use super::variations::ItemVariationStore;

#[cfg(test)]
#[path = "../tests/test_gdef.rs"]
mod tests;

include!("../../generated/generated_gdef.rs");
#[cfg(feature = "sanitize")]
include!("../../generated/generated_gdef_sanitize.rs");

#[cfg(feature = "sanitize")]
pub use super::layout::{
    ClassDefSanitized, CoverageTableSanitized, DeviceOrVariationIndexSanitized,
};

/// temporary type to stand in for tables that will need manual impls
#[cfg(feature = "sanitize")]
pub struct ItemVariationStoreSanitized<'a> {
    ptr: FontPtr<'a>,
}

#[cfg(feature = "sanitize")]
unsafe impl<'a> ReadSanitized<'a> for ItemVariationStoreSanitized<'a> {
    type Args = ();

    unsafe fn read_sanitized(ptr: FontPtr<'a>, _args: &Self::Args) -> Self {
        Self { ptr }
    }
}
