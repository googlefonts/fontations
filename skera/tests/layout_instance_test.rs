//! Condition signs and positioning values use HarfBuzz's distinct precision.
//! Table references were generated with HarfBuzz c82300aefb using the equivalent
//! synthetic font, all glyphs, retained IDs, and the axis locations below.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontData, FontRead, FontRef, TableProvider},
    tables::{gdef::*, gpos::*, gsub::*, layout::*, math::*, variations::*},
    types::{F2Dot14, GlyphId16, Tag},
    FontBuilder,
};

fn coverage() -> CoverageTable {
    CoverageTable::from_iter([GlyphId16::new(1)])
}
fn scripts() -> ScriptList {
    ScriptList::new(vec![ScriptRecord::new(
        Tag::new(b"DFLT"),
        Script::new(Some(LangSys::new(vec![0])), vec![]),
    )])
}
fn synthetic_font(index: u32, default: i16) -> Vec<u8> {
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let source = FontRef::new(&bytes).unwrap();
    let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
    let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
    let neg = RegionAxisCoordinates::new(F2Dot14::NEG_ONE, F2Dot14::NEG_ONE, F2Dot14::ZERO);
    let rows: [[i32; 4]; 7] = [
        [1, 0, 0, 0],
        [-1, 0, 0, 0],
        [i32::MAX, i32::MIN, 0, 0],
        [i32::MAX, 1 - i32::MAX, 0, 0],
        [1, 0, 0, 0],
        [0, 0, -1, 0],
        [0, 0, 0, -2],
    ];
    let store = ItemVariationStore {
        variation_region_list: VariationRegionList::new(
            2,
            vec![
                VariationRegion::new(vec![pos.clone(), zero.clone()]),
                VariationRegion::new(vec![pos.clone(), zero.clone()]),
                VariationRegion::new(vec![zero, neg.clone()]),
                VariationRegion::new(vec![pos, neg]),
            ],
        )
        .into(),
        item_variation_data: vec![ItemVariationData {
            item_count: rows.len() as u16,
            word_delta_count: 0x8004,
            region_indexes: vec![0, 1, 2, 3],
            delta_sets: rows
                .into_iter()
                .flatten()
                .flat_map(i32::to_be_bytes)
                .collect(),
        }
        .into()],
    };
    let gdef = Gdef {
        item_var_store: store.into(),
        lig_caret_list: LigCaretList::new(
            coverage(),
            vec![LigGlyph::new(vec![CaretValue::format_3(
                100,
                VariationIndex::new(0, 1).into(),
            )])],
        )
        .into(),
        ..Default::default()
    };
    let mut gsub = Gsub::new(
        scripts(),
        FeatureList::new(vec![FeatureRecord::new(
            Tag::new(b"liga"),
            Feature::new(None, vec![0]),
        )]),
        SubstitutionLookupList::new(
            [2, 3]
                .into_iter()
                .map(|g| {
                    SubstitutionLookup::Single(Lookup::new(
                        LookupFlag::empty(),
                        vec![SingleSubst::format_2(coverage(), vec![GlyphId16::new(g)])],
                    ))
                })
                .collect(),
        ),
    );
    gsub.feature_variations = FeatureVariations {
        feature_variation_records: vec![FeatureVariationRecord {
            condition_set: ConditionSet {
                conditions: vec![Condition::format_2_variable_value(default, index).into()],
            }
            .into(),
            feature_table_substitution: FeatureTableSubstitution {
                substitutions: vec![FeatureTableSubstitutionRecord {
                    feature_index: 0,
                    alternate_feature: Feature::new(None, vec![1]).into(),
                }],
            }
            .into(),
        }],
    }
    .into();
    let gpos = Gpos::new(
        scripts(),
        FeatureList::new(vec![FeatureRecord::new(
            Tag::new(b"kern"),
            Feature::new(None, (0..6).collect()),
        )]),
        LookupList::new(
            (1..=6)
                .map(|index| {
                    PositionLookup::Single(Lookup::new(
                        LookupFlag::empty(),
                        vec![SinglePosFormat1::new(
                            coverage(),
                            ValueRecord::new()
                                .with_x_advance(100)
                                .with_x_advance_device(VariationIndex::new(0, index)),
                        )
                        .into()],
                    ))
                })
                .collect(),
        ),
    );
    let mut math = Math::default();
    math.math_constants.math_leading =
        MathValueRecord::new(100.into(), Some(VariationIndex::new(0, 1).into()));
    math.math_constants.radical_kern_after_degree =
        MathValueRecord::new(100.into(), Some(VariationIndex::new(0, 6).into()));
    let mut builder = FontBuilder::new();
    for r in source.table_directory().table_records() {
        // Linear normalization makes weight 650 an exact half-unit case.
        if r.tag() == Tag::new(b"avar") {
            continue;
        }
        builder.add_raw(r.tag(), source.data_for_tag(r.tag()).unwrap());
    }
    builder.add_table(&gdef).unwrap();
    builder.add_table(&gsub).unwrap();
    builder.add_table(&gpos).unwrap();
    builder.add_table(&math).unwrap();
    builder.build()
}
fn subset(bytes: &[u8]) -> Vec<u8> {
    let font = FontRef::new(bytes).unwrap();
    let plan = Plan::new(
        &skera::populate_gids("*").unwrap(),
        &IntSet::empty(),
        &font,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
    );
    subset_font(&font, &plan).unwrap()
}
fn selected_glyph(bytes: &[u8]) -> u16 {
    let font = FontRef::new(bytes).unwrap();
    let table: Gsub = font.gsub().unwrap().to_owned_table();
    assert!(table.feature_variations.is_none());
    let index = table.feature_list.feature_records[0]
        .feature
        .lookup_list_indices[0] as usize;
    let SubstitutionLookup::Single(lookup) = table.lookup_list.lookups[index].as_ref() else {
        panic!()
    };
    match lookup.subtables[0].as_ref() {
        SingleSubst::Format1(sub) => 1u16.wrapping_add(sub.delta_glyph_id as u16),
        SingleSubst::Format2(sub) => sub.substitute_glyph_ids[0].to_u16(),
    }
}
#[test]
fn full_instances_match_harfbuzz_for_small_and_cancelling_deltas() {
    for (index, default) in [(0, 0), (2, 1), (3, 0)] {
        let source = synthetic_font(index, default);
        for weight in [525, 650, 900] {
            let bytes = instance_font(
                &FontRef::new(&source).unwrap(),
                &parse_axis_limits(&format!("wght={weight},wdth=87.5")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                selected_glyph(&bytes),
                if index == 2 && weight == 900 { 2 } else { 3 }
            );
            let bytes = subset(&bytes);
            let font = FontRef::new(&bytes).unwrap();
            for tag in [b"GSUB", b"GPOS", b"GDEF"] {
                let reference = std::fs::read(format!(
                    "test-data/expected/layout-instance/{index}-{weight}-{}.bin",
                    String::from_utf8_lossy(tag)
                ))
                .unwrap();
                match tag {
                    b"GSUB" => {
                        let expected: Gsub =
                            write_fonts::read::tables::gsub::Gsub::read(FontData::new(&reference))
                                .unwrap()
                                .to_owned_table();
                        let actual: Gsub = font.gsub().unwrap().to_owned_table();
                        assert_eq!(actual, expected, "index={index} weight={weight}");
                    }
                    b"GPOS" => {
                        let expected: Gpos =
                            write_fonts::read::tables::gpos::Gpos::read(FontData::new(&reference))
                                .unwrap()
                                .to_owned_table();
                        let actual: Gpos = font.gpos().unwrap().to_owned_table();
                        assert_eq!(actual, expected, "index={index} weight={weight}");
                    }
                    b"GDEF" => {
                        let expected: Gdef =
                            write_fonts::read::tables::gdef::Gdef::read(FontData::new(&reference))
                                .unwrap()
                                .to_owned_table();
                        let actual: Gdef = font.gdef().unwrap().to_owned_table();
                        assert_eq!(actual, expected, "index={index} weight={weight}");
                    }
                    _ => unreachable!(),
                }
            }
            // HarfBuzz drops MATH variation devices without folding their
            // deltas. Check these values against the matching GPOS rows,
            // whose results have just been verified against HarfBuzz.
            let gpos: Gpos = font.gpos().unwrap().to_owned_table();
            let advance = |i: usize| {
                let PositionLookup::Single(lookup) = gpos.lookup_list.lookups[i].as_ref() else {
                    panic!()
                };
                let SinglePos::Format1(pos) = lookup.subtables[0].as_ref() else {
                    panic!()
                };
                pos.value_record.x_advance.unwrap()
            };
            let math = font.math().unwrap();
            let constants = math.math_constants().unwrap();
            for (record, expected) in [
                (constants.math_leading(), advance(0)),
                (constants.radical_kern_after_degree(), advance(5)),
            ] {
                assert_eq!(record.value().to_i16(), expected);
                assert!(record.device(constants.offset_data()).is_none());
            }
        }
    }
}
#[test]
fn partial_instancing_folds_constant_conditions_using_their_unrounded_sign() {
    for (index, default) in [(0, 0), (2, 1), (3, 0)] {
        let source = synthetic_font(index, default);
        for weight in [525, 650, 900] {
            let bytes = instance_font(
                &FontRef::new(&source).unwrap(),
                &parse_axis_limits(&format!("wght={weight}")).unwrap(),
            )
            .unwrap();
            let font = FontRef::new(&bytes).unwrap();
            let vars = font.gsub().unwrap().feature_variations().unwrap().unwrap();
            let expected = index != 2 || weight != 900;
            assert_eq!(
                vars.feature_variation_records().len(),
                usize::from(expected)
            );
            if expected {
                let record = &vars.feature_variation_records()[0];
                assert_eq!(
                    record
                        .condition_set(vars.offset_data())
                        .unwrap()
                        .unwrap()
                        .condition_count(),
                    0
                );
            }
            let bytes = subset(&bytes);
            let bytes = instance_font(
                &FontRef::new(&bytes).unwrap(),
                &parse_axis_limits("wdth=87.5").unwrap(),
            )
            .unwrap();
            assert_eq!(selected_glyph(&bytes), if expected { 3 } else { 2 });
        }
    }
}

#[test]
fn partial_instances_drop_devices_whose_residual_deltas_vanish() {
    let source = synthetic_font(0, 0);
    for weight in [525, 650, 900] {
        let bytes = instance_font(
            &FontRef::new(&source).unwrap(),
            &parse_axis_limits(&format!("wght={weight}")).unwrap(),
        )
        .unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let gpos: Gpos = font.gpos().unwrap().to_owned_table();
        for (i, lookup) in gpos.lookup_list.lookups.iter().enumerate() {
            let PositionLookup::Single(lookup) = lookup.as_ref() else {
                panic!()
            };
            let SinglePos::Format1(pos) = lookup.subtables[0].as_ref() else {
                panic!()
            };
            // Weight-only rows are constant. The two-axis row also vanishes
            // at weight 525, since its residual -0.5 rounds to zero.
            let variable = i == 4 || (i == 5 && weight != 525);
            assert_eq!(pos.value_record.x_advance_device.is_some(), variable);
            assert_eq!(
                pos.value_record.format(),
                ValueFormat::X_ADVANCE
                    | if variable {
                        ValueFormat::X_ADVANCE_DEVICE
                    } else {
                        ValueFormat::empty()
                    }
            );
        }
        let gdef: Gdef = font.gdef().unwrap().to_owned_table();
        assert_eq!(
            gdef.lig_caret_list.as_ref().unwrap().lig_glyphs[0].caret_values[0].as_ref(),
            &CaretValue::format_1(if weight == 900 { 99 } else { 100 })
        );
        if weight == 525 {
            let bytes = subset(&bytes);
            let font = FontRef::new(&bytes).unwrap();
            for tag in [b"GPOS", b"GDEF"] {
                let reference = std::fs::read(format!(
                    "test-data/expected/layout-instance/partial-525-{}.bin",
                    String::from_utf8_lossy(tag)
                ))
                .unwrap();
                if tag == b"GPOS" {
                    let expected: Gpos =
                        write_fonts::read::tables::gpos::Gpos::read(FontData::new(&reference))
                            .unwrap()
                            .to_owned_table();
                    let actual: Gpos = font.gpos().unwrap().to_owned_table();
                    assert_eq!(actual, expected);
                } else {
                    let expected: Gdef =
                        write_fonts::read::tables::gdef::Gdef::read(FontData::new(&reference))
                            .unwrap()
                            .to_owned_table();
                    let actual: Gdef = font.gdef().unwrap().to_owned_table();
                    assert_eq!(actual, expected);
                }
            }
        }
    }
}

#[test]
fn partial_positioning_values_match_harfbuzz_float_precision() {
    let source = synthetic_font(0, 0);
    for weight in [650, 900] {
        let instance = instance_font(
            &FontRef::new(&source).unwrap(),
            &parse_axis_limits(&format!("wght={weight}")).unwrap(),
        )
        .unwrap();
        let font = FontRef::new(&instance).unwrap();
        let actual: Gpos = font.gpos().unwrap().to_owned_table();
        let reference = std::fs::read(format!(
            "test-data/expected/layout-instance/partial-{weight}-GPOS.bin"
        ))
        .unwrap();
        let expected: Gpos = write_fonts::read::tables::gpos::Gpos::read(FontData::new(&reference))
            .unwrap()
            .to_owned_table();
        // Compare the weight-only rows, which are constant after this pin.
        // Residual store optimization can remap other rows' device indices.
        for i in 0..4 {
            assert_eq!(
                actual.lookup_list.lookups[i],
                expected.lookup_list.lookups[i]
            );
        }
        for width in [100., 87.5, 75.] {
            let second =
                instance_font(&font, &parse_axis_limits(&format!("wdth={width}")).unwrap())
                    .unwrap();
            let second = FontRef::new(&second).unwrap();
            let second: Gpos = second.gpos().unwrap().to_owned_table();
            for i in 0..4 {
                assert_eq!(
                    second.lookup_list.lookups[i],
                    expected.lookup_list.lookups[i]
                );
            }
        }
    }
}

#[test]
fn partial_conditions_keep_fractional_sign_boundaries_without_changing_positions() {
    for (gain, delta) in [(0i32, -1i32), (0, 1), (1, -2)] {
        let original = synthetic_font(6, 0);
        let font = FontRef::new(&original).unwrap();
        let mut gdef: Gdef = font.gdef().unwrap().to_owned_table();
        let data = gdef.item_var_store.as_mut().unwrap().item_variation_data[0]
            .as_mut()
            .unwrap();
        data.delta_sets[6 * 16..6 * 16 + 4].copy_from_slice(&gain.to_be_bytes());
        data.delta_sets[6 * 16 + 12..7 * 16].copy_from_slice(&delta.to_be_bytes());
        let mut builder = FontBuilder::new();
        for record in font.table_directory().table_records() {
            builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
        }
        builder.add_table(&gdef).unwrap();
        let source = builder.build();
        for weight in [525, 650, 900] {
            let partial = instance_font(
                &FontRef::new(&source).unwrap(),
                &parse_axis_limits(&format!("wght={weight}")).unwrap(),
            )
            .unwrap();
            let partial = subset(&partial);
            let font = FontRef::new(&partial).unwrap();
            let variations = font.gsub().unwrap().feature_variations().unwrap().unwrap();
            assert_eq!(variations.feature_variation_record_count(), 1);
            let set = variations.feature_variation_records()[0]
                .condition_set(variations.offset_data())
                .unwrap()
                .unwrap();
            assert_eq!(set.condition_count(), 1);
            for width in [100, 99, 87, 75] {
                let second = instance_font(
                    &FontRef::new(&partial).unwrap(),
                    &parse_axis_limits(&format!("wdth={width}")).unwrap(),
                )
                .unwrap();
                let direct = instance_font(
                    &FontRef::new(&source).unwrap(),
                    &parse_axis_limits(&format!("wght={weight},wdth={width}")).unwrap(),
                )
                .unwrap();
                assert_eq!(
                    selected_glyph(&second),
                    selected_glyph(&direct),
                    "gain={gain} delta={delta} weight={weight} width={width}",
                );
                let position = |bytes: &[u8]| {
                    let font = FontRef::new(bytes).unwrap();
                    let table: Gpos = font.gpos().unwrap().to_owned_table();
                    let PositionLookup::Single(lookup) = table.lookup_list.lookups[5].as_ref()
                    else {
                        panic!()
                    };
                    let SinglePos::Format1(pos) = lookup.subtables[0].as_ref() else {
                        panic!()
                    };
                    pos.value_record.x_advance
                };
                // Positioning rounds at each instancing stage. At weight 525
                // the residual +/-0.25 vanishes, while the condition varies.
                if weight == 525 && gain == 0 {
                    assert_eq!(position(&second), Some(100));
                    assert_eq!(position(&direct), Some(100));
                }
            }
        }
    }
}
