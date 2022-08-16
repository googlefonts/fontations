//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

use font_types::MajorMinor;

use super::{ClassDef, CoverageTable, Device};

include!("../../generated/generated_gdef.rs");

impl Gdef {
    fn compute_version(&self) -> MajorMinor {
        if self.item_var_store_offset.is_some() {
            MajorMinor::VERSION_1_3
        } else if self.mark_glyph_sets_def_offset.is_some() {
            MajorMinor::VERSION_1_2
        } else {
            MajorMinor::VERSION_1_0
        }
    }
}
