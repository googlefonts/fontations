use super::*;
use read_fonts::{types::GlyphId, FontData, FontRead};
use write_fonts::tables::{gpos::*, layout::*};

fn coverage() -> CoverageTable {
    [GlyphId::new(65536)].into_iter().collect()
}

fn wide_lookups(value: ValueRecord, anchor: AnchorTable) -> Vec<PositionLookup> {
    let flags = LookupFlag::empty();
    let marks = || MarkArray2::new(vec![MarkRecord2::new(0, anchor.clone())]);
    vec![
        PositionLookup::Single(Lookup::new(
            flags,
            vec![SinglePosFormat3::new(coverage(), value.clone()).into()],
        )),
        PositionLookup::Single(Lookup::new(
            flags,
            vec![SinglePosFormat4::new(coverage(), vec![value.clone()]).into()],
        )),
        PositionLookup::Pair(Lookup::new(
            flags,
            vec![PairPosFormat3::new(
                coverage(),
                vec![PairSet2::new(vec![PairValueRecord2::new(
                    read_fonts::types::GlyphId24::new(65537),
                    value.clone(),
                    value.clone(),
                )])],
            )
            .into()],
        )),
        PositionLookup::Pair(Lookup::new(
            flags,
            vec![PairPosFormat4::new(
                coverage(),
                ClassDef::default(),
                ClassDef::default(),
                vec![Class1Record::new(vec![Class2Record::new(
                    value.clone(),
                    value.clone(),
                )])],
            )
            .into()],
        )),
        PositionLookup::Cursive(Lookup::new(
            flags,
            vec![CursivePosFormat2::new(
                coverage(),
                vec![EntryExitRecord2::new(
                    Some(anchor.clone()),
                    Some(anchor.clone()),
                )],
            )
            .into()],
        )),
        PositionLookup::MarkToBase(Lookup::new(
            flags,
            vec![MarkBasePosFormat2::new(
                coverage(),
                coverage(),
                marks(),
                BaseArray2::new(vec![BaseRecord2::new(vec![Some(anchor.clone())])]),
            )
            .into()],
        )),
        PositionLookup::MarkToLig(Lookup::new(
            flags,
            vec![MarkLigPosFormat2::new(
                coverage(),
                coverage(),
                marks(),
                LigatureArray2::new(vec![LigatureAttach2::new(vec![ComponentRecord2::new(
                    vec![Some(anchor.clone())],
                )])]),
            )
            .into()],
        )),
        PositionLookup::MarkToMark(Lookup::new(
            flags,
            vec![MarkMarkPosFormat2::new(
                coverage(),
                coverage(),
                marks(),
                Mark2Array2::new(vec![Mark2Record2::new(vec![Some(anchor.clone())])]),
            )
            .into()],
        )),
    ]
}

#[test]
fn every_wide_positioning_format_instances_and_remaps_values() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
    let store = super::tests::store_bytes(&[[10, 20]]);
    let context = LayoutContext::new(
        Some(ItemVariationStore::read(FontData::new(&store)).unwrap()),
        &axes,
    );
    let value = |base, outer, inner| {
        ValueRecord::new()
            .with_x_advance(base)
            .with_x_advance_device(VariationIndex::new(outer, inner))
    };
    let anchor = |base, outer, inner| {
        AnchorTable::format_3(
            base,
            20,
            Some(VariationIndex::new(outer, inner).into()),
            None,
        )
    };
    let mut actual = wide_lookups(value(100, 0, 0), anchor(100, 0, 0));
    actual.apply(&ValuePass::Instance(&context)).unwrap();
    actual
        .apply(&ValuePass::Remap(&[(0, 0x0001_0002)].into_iter().collect()))
        .unwrap();
    let expected = wide_lookups(value(110, 1, 2), anchor(110, 1, 2));
    for (actual, expected) in actual.iter().zip(&expected) {
        assert_eq!(
            write_fonts::dump_table(actual).unwrap(),
            write_fonts::dump_table(expected).unwrap()
        );
    }
}

#[test]
fn wide_header_keeps_empty_single_counts_and_pair_matrix_dimensions() {
    let mut table = Gpos {
        script_list2: Some(ScriptList::default()).into(),
        feature_list2: Some(FeatureList::default()).into(),
        lookup_list2: Some(PositionLookupList2::new(vec![
            PositionLookup::Single(Lookup::new(
                LookupFlag::empty(),
                vec![SinglePosFormat4::new(
                    [GlyphId::new(65536), GlyphId::new(65537)]
                        .into_iter()
                        .collect(),
                    vec![ValueRecord::new(); 2],
                )
                .into()],
            )),
            PositionLookup::Pair(Lookup::new(
                LookupFlag::empty(),
                vec![PairPosFormat4::new(
                    coverage(),
                    ClassDef::default(),
                    ClassDef::default(),
                    vec![
                        Class1Record::new(vec![
                            Class2Record::new(
                                ValueRecord::new(),
                                ValueRecord::new()
                            );
                            5
                        ]);
                        3
                    ],
                )
                .into()],
            )),
        ]))
        .into(),
        ..Default::default()
    };
    let bytes = write_fonts::dump_table(&table).unwrap();
    let source = read_fonts::tables::gpos::Gpos::read(FontData::new(&bytes)).unwrap();
    let owned = own_gpos(&source).unwrap();
    assert!(owned.lookup_list.is_none());
    assert!(owned.feature_list.is_none());
    assert!(owned.script_list.is_none());
    assert_eq!(write_fonts::dump_table(&owned).unwrap(), {
        table.lookup_list = Default::default();
        table.feature_list = Default::default();
        table.script_list = Default::default();
        write_fonts::dump_table(&table).unwrap()
    });
}

#[test]
fn lookup_variations_survive_partial_instancing_and_replace_after_feature_variations() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let list = FeatureList::new(vec![FeatureRecord::new(
        Tag::new(b"kern"),
        Feature::new(None, vec![0]),
    )]);
    let store = super::tests::store_bytes(&[[10, 20]]);
    for add_defaults in [false, true] {
        let mut vars = FeatureVariations::new(vec![FeatureVariationRecord::new(
            None,
            Some(FeatureTableSubstitution::new(vec![
                FeatureTableSubstitutionRecord::new(0, Feature::new(None, vec![3])),
            ])),
        )]);
        vars.lookup_variation_records = Some(vec![LookupVariationRecord::new(
            0,
            FeatureLookups::new(
                if add_defaults {
                    FeatureLookupsFlags::ADD_DEFAULT_LOOKUPS
                } else {
                    FeatureLookupsFlags::empty()
                },
                vec![
                    LookupConditionRecord::new(
                        Some(Condition::format_1_axis_range(
                            0,
                            F2Dot14::from_f32(0.5),
                            F2Dot14::ONE,
                        )),
                        LookupIndexList::new(vec![1]),
                    ),
                    LookupConditionRecord::new(
                        Some(Condition::format_1_axis_range(
                            1,
                            F2Dot14::from_f32(0.5),
                            F2Dot14::ONE,
                        )),
                        LookupIndexList::new(vec![2]),
                    ),
                    LookupConditionRecord::new(
                        Some(Condition::format_2_variable_value(-10, 0)),
                        LookupIndexList::new(vec![4]),
                    ),
                    LookupConditionRecord::new(None, LookupIndexList::new(vec![5, 5])),
                ],
            ),
        )]);
        let data = write_fonts::dump_table(&vars).unwrap();
        let features_data = write_fonts::dump_table(&list).unwrap();
        let read_features =
            read_fonts::tables::layout::FeatureList::read(FontData::new(&features_data)).unwrap();
        let original = own_feature_variations(
            Some(
                read_fonts::tables::layout::FeatureVariations::read(FontData::new(&data)).unwrap(),
            ),
            &read_features,
            Tag::new(b"GPOS"),
        )
        .unwrap()
        .unwrap();
        for (limits, full) in [("wght=900", false), ("wght=900,CNTR=100", true)] {
            let axes = AxisPlan::new(&font, &crate::parse_axis_limits(limits).unwrap()).unwrap();
            let context = LayoutContext::new(
                Some(ItemVariationStore::read(FontData::new(&store)).unwrap()),
                &axes,
            );
            let mut list = list.clone();
            let mut vars = Some(original.clone()).into();
            instance_features(&mut list, &mut vars, &context).unwrap();
            if full {
                assert!(vars.is_none());
                assert_eq!(
                    list.feature_records[0].feature.lookup_list_indices,
                    if add_defaults {
                        vec![1, 2, 3, 4, 5]
                    } else {
                        vec![1, 2, 4, 5]
                    }
                );
            } else {
                let records = &vars
                    .as_ref()
                    .unwrap()
                    .lookup_variation_records
                    .as_ref()
                    .unwrap()[0]
                    .feature_lookups
                    .lookup_condition_records;
                assert!(records[0].condition.is_none());
                assert!(
                    matches!(records[1].condition.as_ref(), Some(Condition::Format1AxisRange(c)) if c.axis_index == 0)
                );
                let Condition::Format2VariableValue(c) = records[2].condition.as_ref().unwrap()
                else {
                    panic!()
                };
                let map = [(c.var_index, 0x0002_0003)].into_iter().collect();
                remap_feature_conditions(&mut vars, &map);
                let records = &vars
                    .as_ref()
                    .unwrap()
                    .lookup_variation_records
                    .as_ref()
                    .unwrap()[0]
                    .feature_lookups
                    .lookup_condition_records;
                assert!(
                    matches!(records[2].condition.as_ref(), Some(Condition::Format2VariableValue(c)) if c.var_index == 0x0002_0003)
                );
            }
        }
    }
}

#[test]
fn empty_pair_matrices_charge_rows_even_without_columns() {
    let rows = vec![
        Class1Record::new(vec![
            Class2Record::new(
                ValueRecord::new(),
                ValueRecord::new()
            );
            2
        ]);
        2
    ];
    let coverage: CoverageTable = [GlyphId::new(1)].into_iter().collect();
    for (pair, count_offset) in [
        (
            PairPos::from(PairPosFormat2::new(
                coverage.clone(),
                ClassDef::default(),
                ClassDef::default(),
                rows.clone(),
            )),
            12,
        ),
        (
            PairPos::from(PairPosFormat4::new(
                coverage.clone(),
                ClassDef::default(),
                ClassDef::default(),
                rows.clone(),
            )),
            18,
        ),
    ] {
        let mut bytes = write_fonts::dump_table(&pair).unwrap();
        bytes[count_offset + 2..count_offset + 4].copy_from_slice(&0u16.to_be_bytes());
        let source = read_fonts::tables::gpos::PairPos::read(FontData::new(&bytes)).unwrap();
        let mut target = pair.clone();
        let mut remaining = 1;
        assert!(
            super::owned::own_empty_pairs(source.clone(), &mut target, &mut remaining).is_err()
        );
        assert_eq!(remaining, 1);
        let mut remaining = 2;
        super::owned::own_empty_pairs(source, &mut target, &mut remaining).unwrap();
        assert_eq!(remaining, 0);
        let records = match &target {
            PairPos::Format2(table) => &table.class1_records,
            PairPos::Format4(table) => &table.class1_records,
            _ => unreachable!(),
        };
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|row| row.class2_records.is_empty()));
        bytes[count_offset..count_offset + 2].copy_from_slice(&0u16.to_be_bytes());
        bytes[count_offset + 2..count_offset + 4].copy_from_slice(&u16::MAX.to_be_bytes());
        let source = read_fonts::tables::gpos::PairPos::read(FontData::new(&bytes)).unwrap();
        let mut remaining = 1;
        super::owned::own_empty_pairs(source, &mut target, &mut remaining).unwrap();
        assert_eq!(remaining, 1);
        let records = match &target {
            PairPos::Format2(table) => &table.class1_records,
            PairPos::Format4(table) => &table.class1_records,
            _ => unreachable!(),
        };
        assert!(records.is_empty());
    }
}
