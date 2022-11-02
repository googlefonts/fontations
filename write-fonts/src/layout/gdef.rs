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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_store_without_glyph_sets() {
        // this should compile, and version should be 1.3
        let gdef = Gdef {
            glyph_class_def_offset: NullableOffsetMarker::new(None),
            attach_list_offset: NullableOffsetMarker::new(None),
            lig_caret_list_offset: NullableOffsetMarker::new(None),
            mark_attach_class_def_offset: NullableOffsetMarker::new(None),
            mark_glyph_sets_def_offset: NullableOffsetMarker::new(None),
            item_var_store_offset: NullableOffsetMarker::new(Some(ClassDef::Format1(
                crate::layout::ClassDefFormat1 {
                    start_glyph_id: GlyphId::new(2),
                    class_value_array: vec![1, 2, 0],
                },
            ))),
        };

        assert_eq!(gdef.compute_version(), MajorMinor::VERSION_1_3);
        let _dumped = crate::write::dump_table(&gdef).unwrap();
        let data = FontData::new(&_dumped);
        let loaded = read_fonts::tables::gdef::Gdef::read(data).unwrap();

        assert_eq!(loaded.version(), MajorMinor::VERSION_1_3);
        assert!(!loaded.item_var_store_offset().unwrap().is_null());
    }
}
