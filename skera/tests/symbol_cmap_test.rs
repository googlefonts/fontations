//! Symbol cmap retention and low-byte PUA aliases, from HB report 51.
use skera::{subset_font, Plan, SubsetFlags, DEFAULT_LAYOUT_FEATURES};
use skrifa::MetadataProvider;
use write_fonts::{
    read::{collections::IntSet, FontRef, TableProvider},
    types::GlyphId,
};

#[test]
fn symbol_maps_survive_unicode_and_glyph_selection() {
    for bytes in [
        include_bytes!("../test-data/fonts/symbol-cmap4.ttf").as_slice(),
        include_bytes!("../test-data/fonts/symbol-cmap12.ttf").as_slice(),
    ] {
        let font = FontRef::new(bytes).unwrap();
        for case in 0..5 {
            let mut unicodes = IntSet::empty();
            let mut gids = IntSet::empty();
            match case {
                0 => {
                    unicodes.insert(0x41);
                }
                1 => {
                    unicodes.insert(0xF041);
                }
                2 => {
                    unicodes.extend([0x41, 0xF042]);
                }
                3 => {
                    unicodes = IntSet::all();
                    unicodes.remove(0xF041);
                }
                _ => {
                    gids.insert(GlyphId::new(1));
                }
            }
            let plan = Plan::new(
                &gids,
                &unicodes,
                &font,
                SubsetFlags::default(),
                &IntSet::empty(),
                &IntSet::all(),
                &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
                &IntSet::all(),
                &IntSet::all(),
            );
            let mapping = plan
                .old_to_new_glyph_mapping()
                .collect::<std::collections::BTreeMap<_, _>>();
            let bytes = subset_font(&font, &plan).unwrap();
            let subset = FontRef::new(&bytes).unwrap();
            let cmap = subset.cmap().expect("Symbol cmap was dropped");
            assert!(cmap.encoding_records().iter().any(|r| r.platform_id()
                == write_fonts::read::tables::cmap::PlatformId::Windows
                && r.encoding_id() == 0));
            let map = subset.charmap();
            assert!(map.is_symbol());
            if case == 3 {
                assert_eq!(map.map(0xF041u32), None);
                assert_eq!(map.map(0x41u32), None);
            } else {
                assert_eq!(map.map(0x41u32), mapping.get(&GlyphId::new(1)).copied());
                assert_eq!(map.map(0xF041u32), mapping.get(&GlyphId::new(1)).copied());
            }
            if case == 2 || case == 3 {
                assert_eq!(map.map(0x42u32), mapping.get(&GlyphId::new(2)).copied());
            }
        }
    }
}

#[test]
fn direct_low_byte_mapping_does_not_retain_a_different_pua_glyph() {
    for bytes in [
        include_bytes!("../test-data/fonts/symbol-direct-cmap4.ttf").as_slice(),
        include_bytes!("../test-data/fonts/symbol-direct-cmap12.ttf").as_slice(),
    ] {
        let font = FontRef::new(bytes).unwrap();
        let plan = Plan::new(
            &IntSet::empty(),
            &[0x41].into_iter().collect(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
            &IntSet::all(),
            &IntSet::all(),
        );
        let mapping = plan
            .old_to_new_glyph_mapping()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(!mapping.contains_key(&GlyphId::new(1)));
        let bytes = subset_font(&font, &plan).unwrap();
        let subset = FontRef::new(&bytes).unwrap();
        assert_eq!(
            subset.charmap().map(0x41u32),
            mapping.get(&GlyphId::new(2)).copied()
        );
        assert_eq!(subset.charmap().map(0xF041u32), None);
    }
}
