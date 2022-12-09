//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

use types::MajorMinor;

use super::{
    layout::{ClassDef, CoverageTable, Device},
    variations::ItemVariationStore,
};

include!("../../generated/generated_gdef.rs");

impl Gdef {
    fn compute_version(&self) -> MajorMinor {
        if self.item_var_store.is_some() {
            MajorMinor::VERSION_1_3
        } else if self.mark_glyph_sets_def.is_some() {
            MajorMinor::VERSION_1_2
        } else {
            MajorMinor::VERSION_1_0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_store_without_glyph_sets() {
        // this should compile, and version should be 1.3
        let gdef = Gdef {
            item_var_store: ItemVariationStore::default().into(),
            ..Default::default()
        };

        assert_eq!(gdef.compute_version(), MajorMinor::VERSION_1_3);
        let _dumped = crate::write::dump_table(&gdef).unwrap();
        let data = FontData::new(&_dumped);
        let loaded = read_fonts::tables::gdef::Gdef::read(data).unwrap();

        assert_eq!(loaded.version(), MajorMinor::VERSION_1_3);
        assert!(!loaded.item_var_store_offset().unwrap().is_null());
    }
}
