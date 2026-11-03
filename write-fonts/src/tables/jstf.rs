//! The [JSTF](https://learn.microsoft.com/en-us/typography/opentype/spec/jstf) table.

/// Positioning lookups embedded in JSTF, whose offsets are always 16-bit.
pub type JstfMax = super::gpos::OriginalPositionLookupList;

include!("../../generated/generated_jstf.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        from_obj::ToOwnedTable,
        tables::{
            gpos::{PositionLookup, SinglePos, ValueRecord},
            layout::{CoverageTable, Lookup, LookupFlag},
        },
    };

    #[test]
    fn scripts_languages_priorities_and_embedded_gpos_roundtrip() {
        let lookup = PositionLookup::Single(Lookup::new(
            LookupFlag::default(),
            vec![SinglePos::format_1(
                CoverageTable::format_1(vec![GlyphId16::new(3)]),
                ValueRecord::new().with_x_advance(25),
            )],
        ));
        let priority = JstfPriority {
            gsub_shrinkage_enable: Some(JstfModList::new(vec![2, 5])).into(),
            extension_jstf_max: Some(JstfMax::new(vec![lookup])).into(),
            ..Default::default()
        };
        let language = JstfLangSys::new(vec![priority]);
        let table = Jstf::new(vec![JstfScriptRecord::new(
            Tag::new(b"arab"),
            JstfScript::new(
                Some(ExtenderGlyph::new(vec![GlyphId16::new(7)])),
                Some(language.clone()),
                vec![JstfLangSysRecord::new(Tag::new(b"URD "), language)],
            ),
        )]);
        let bytes = crate::dump_table(&table).unwrap();
        let read = read_fonts::tables::jstf::Jstf::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.version(), MajorMinor::VERSION_1_0);
        let script = read.jstf_script_records()[0]
            .jstf_script(read.offset_data())
            .unwrap();
        let priorities = script.def_jstf_lang_sys().unwrap().unwrap();
        let priority = priorities.jstf_priorities().get(0).unwrap();
        assert_eq!(
            priority
                .gsub_shrinkage_enable()
                .unwrap()
                .unwrap()
                .lookup_indices(),
            &[2u16, 5u16]
        );
        assert_eq!(
            priority
                .extension_jstf_max()
                .unwrap()
                .unwrap()
                .lookup_count(),
            1
        );
        let owned: Jstf = read.to_owned_table();
        assert_eq!(crate::dump_table(&owned).unwrap(), bytes);
    }
}
