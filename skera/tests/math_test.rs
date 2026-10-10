//! HarfBuzz references and targeted tests for MATH closure and device bases.
use skera::{subset_font, Plan, SubsetFlags, DEFAULT_LAYOUT_FEATURES};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontData, FontRead, FontRef, TableProvider},
    tables::{gsub::*, layout::*, math::*},
    types::{GlyphId, GlyphId16, Tag},
    FontBuilder,
};

fn source() -> Vec<u8> {
    std::fs::read("test-data/fonts/STIXTwoMath-Regular.ttf").unwrap()
}

fn plan(font: &FontRef, gids: &str, unicodes: &str, flags: SubsetFlags) -> Plan {
    Plan::new(
        &skera::populate_gids(gids).unwrap(),
        &skera::parse_unicodes(unicodes).unwrap(),
        font,
        flags,
        &IntSet::empty(),
        &IntSet::all(),
        &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
        &IntSet::all(),
        &IntSet::all(),
    )
}

#[test]
fn math_tables_match_harfbuzz() {
    let bytes = source();
    let font = FontRef::new(&bytes).unwrap();
    for (unicodes, count, last_gid) in [
        ("2f,7c,305", 13, 1430),
        ("2211,222b,221a,28,29", 47, 4866),
        ("41,42,1d434", 4, 3301),
    ] {
        for retain in [false, true] {
            let suffix = if retain { "-retain" } else { "" };
            let reference = std::fs::read(format!(
                "test-data/expected/math/{}{suffix}.bin",
                unicodes.replace(',', "-")
            ))
            .unwrap();
            let expected = write_fonts::read::tables::math::Math::read(FontData::new(&reference))
                .unwrap()
                .to_owned_table();
            let flags = SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
                | if retain {
                    SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
                } else {
                    SubsetFlags::default()
                };
            let bytes = subset_font(&font, &plan(&font, "", unicodes, flags)).unwrap();
            let subset = FontRef::new(&bytes).unwrap();
            let actual: Math = subset.math().unwrap().to_owned_table();
            assert_eq!(actual, expected, "unicodes={unicodes} retain={retain}");
            assert_eq!(
                subset.maxp().unwrap().num_glyphs(),
                if retain { last_gid } else { count }
            );
        }
    }
}

fn coverage(gids: &[u16]) -> CoverageTable {
    CoverageTable::from_iter(gids.iter().map(|&g| GlyphId16::new(g)))
}

fn value(value: i16) -> MathValueRecord {
    MathValueRecord::new(
        value.into(),
        Some(Device::new(10, 13, &[-2, -1, 0, 1]).into()),
    )
}

fn construction(variant: u16, part: u16) -> MathGlyphConstruction {
    MathGlyphConstruction::new(
        Some(GlyphAssembly::new(
            value(30),
            vec![GlyphPartRecord::new(
                GlyphId16::new(part),
                20.into(),
                40.into(),
                200.into(),
                PartFlags::EXTENDER_FLAG,
            )],
        )),
        vec![MathGlyphVariantRecord::new(
            GlyphId16::new(variant),
            300.into(),
        )],
    )
}

fn synthetic_font() -> Vec<u8> {
    let bytes = source();
    let font = FontRef::new(&bytes).unwrap();
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
    }
    let constants = MathConstants {
        math_leading: value(12),
        radical_kern_after_degree: value(-12),
        ..Default::default()
    };
    let gids = [2, 5, 6, 8, 10, 11];
    let kern = MathKern::new(vec![value(100)], vec![value(-10), value(20)]);
    let math = Math::new(
        constants,
        MathGlyphInfo::new(
            Some(MathItalicsCorrectionInfo::new(
                coverage(&gids),
                gids.iter().map(|&g| value(g as i16)).collect(),
            )),
            Some(MathTopAccentAttachment::new(
                coverage(&gids),
                gids.iter().map(|&g| value(100 + g as i16)).collect(),
            )),
            Some(coverage(&gids)),
            Some(MathKernInfo::new(
                coverage(&[2, 5]),
                vec![
                    MathKernInfoRecord::new(Some(kern.clone()), None, None, Some(kern.clone())),
                    MathKernInfoRecord::new(None, Some(kern.clone()), Some(kern), None),
                ],
            )),
        ),
        MathVariants::new(
            10.into(),
            Some(coverage(&[2, 5, 8])),
            Some(coverage(&[2])),
            vec![construction(5, 6), construction(7, 7), construction(9, 9)],
            vec![construction(10, 11)],
        ),
    );
    builder.add_table(&math).unwrap();
    let lookup = SubstitutionLookup::Single(Lookup::new(
        LookupFlag::empty(),
        vec![SingleSubstFormat2::new(coverage(&[2]), vec![GlyphId16::new(8)]).into()],
    ));
    builder
        .add_table(&Gsub::new(
            ScriptList::new(vec![ScriptRecord::new(
                Tag::new(b"DFLT"),
                Script::new(Some(LangSys::new(vec![0])), vec![]),
            )]),
            FeatureList::new(vec![FeatureRecord::new(
                Tag::new(b"liga"),
                Feature::new(None, vec![0]),
            )]),
            SubstitutionLookupList::new(vec![lookup]),
        ))
        .unwrap();
    builder.build()
}

#[test]
fn closure_precedes_gsub_and_only_selected_roots_keep_constructions() {
    let bytes = synthetic_font();
    let font = FontRef::new(&bytes).unwrap();
    for retain in [false, true] {
        let flags = if retain {
            SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
        } else {
            SubsetFlags::default()
        };
        let plan = plan(&font, "2", "", flags);
        let mapping = plan
            .old_to_new_glyph_mapping()
            .collect::<std::collections::BTreeMap<_, _>>();
        for gid in [2, 5, 6, 8, 10, 11] {
            assert!(mapping.contains_key(&GlyphId::new(gid)));
        }
        let bytes = subset_font(&font, &plan).unwrap();
        let subset = FontRef::new(&bytes).unwrap();
        let math = subset.math().unwrap();
        let variants = math.math_variants().unwrap();
        let root = GlyphId16::new(mapping[&GlyphId::new(2)].to_u32() as u16);
        for c in [
            variants.vert_glyph_coverage(),
            variants.horiz_glyph_coverage(),
        ] {
            assert_eq!(c.unwrap().unwrap().iter().collect::<Vec<_>>(), [root]);
        }
        let values = math
            .math_glyph_info()
            .unwrap()
            .math_italics_correction_info()
            .unwrap()
            .unwrap();
        let expected =
            [2, 5, 6, 10, 11].map(|g| GlyphId16::new(mapping[&GlyphId::new(g)].to_u32() as u16));
        assert_eq!(
            values.coverage().unwrap().iter().collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            values
                .italics_correction()
                .iter()
                .map(|v| v.value().to_i16())
                .collect::<Vec<_>>(),
            [2, 5, 6, 10, 11]
        );
    }
}

#[test]
fn devices_keep_their_enclosing_table_base() {
    let bytes = synthetic_font();
    let font = FontRef::new(&bytes).unwrap();
    let original: Math = font.math().unwrap().to_owned_table();
    // HarfBuzz copies MATH devices even when hinting is removed elsewhere.
    for flags in [SubsetFlags::default(), SubsetFlags::SUBSET_FLAGS_NO_HINTING] {
        let bytes = subset_font(&font, &plan(&font, "*", "", flags)).unwrap();
        let subset = FontRef::new(&bytes).unwrap();
        let actual: Math = subset.math().unwrap().to_owned_table();
        assert_eq!(actual, original);
        let constants = subset.math().unwrap().math_constants().unwrap();
        assert_eq!(
            constants
                .math_leading()
                .value_for_ppem(constants.offset_data(), 10)
                .delta_px,
            -2
        );
    }
}

#[test]
fn dropping_math_skips_variant_closure() {
    let bytes = synthetic_font();
    let font = FontRef::new(&bytes).unwrap();
    let plan = Plan::new(
        &skera::populate_gids("2").unwrap(),
        &IntSet::empty(),
        &font,
        SubsetFlags::default(),
        &[Tag::new(b"MATH"), Tag::new(b"GSUB")].into_iter().collect(),
        &IntSet::empty(),
        &IntSet::empty(),
        &IntSet::empty(),
        &IntSet::empty(),
    );
    assert!(!plan
        .old_to_new_glyph_mapping()
        .any(|(g, _)| g == GlyphId::new(5)));
    let bytes = subset_font(&font, &plan).unwrap();
    assert!(FontRef::new(&bytes).unwrap().math().is_err());
}

#[test]
fn malformed_required_math_offsets_fail_subsetting() {
    let bytes = synthetic_font();
    let source = FontRef::new(&bytes).unwrap();
    let tag = Tag::new(b"MATH");
    let mut builder = FontBuilder::new();
    for record in source.table_directory().table_records() {
        builder.add_raw(record.tag(), source.data_for_tag(record.tag()).unwrap());
    }
    builder.add_raw(tag, &source.data_for_tag(tag).unwrap().as_bytes()[..10]);
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    assert!(matches!(
        subset_font(&font, &plan(&font, "2", "", SubsetFlags::default())),
        Err(skera::SubsetError::SubsetTableError(t)) if t == tag
    ));
}
