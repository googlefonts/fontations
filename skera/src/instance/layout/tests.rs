use super::features::{condition, partial_condition, PartialCondition};
use super::owned::own_empty_pairs;
use super::*;
use read_fonts::{FontData, FontRead};
use write_fonts::{
    tables::{base::*, gdef::*, gpos::*, layout::*},
    NullableOffsetMarker,
};

pub(super) fn store_bytes(rows: &[[i32; 2]]) -> Vec<u8> {
    use write_fonts::tables::variations::*;
    let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
    let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
    write_fonts::dump_table(&write_fonts::tables::variations::ItemVariationStore {
        variation_region_list: VariationRegionList::new(
            2,
            vec![
                VariationRegion::new(vec![pos.clone(), zero.clone()]),
                VariationRegion::new(vec![zero, pos]),
            ],
        )
        .into(),
        item_variation_data: vec![ItemVariationData {
            item_count: rows.len() as u16,
            word_delta_count: 0x8002,
            region_indexes: vec![0, 1],
            delta_sets: rows
                .iter()
                .flatten()
                .flat_map(|v| v.to_be_bytes())
                .collect(),
        }
        .into()],
    })
    .unwrap()
}

#[test]
fn remapping_preserves_instanced_values_and_only_changes_variation_references() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
    let store = store_bytes(&[[10, 20]]);
    let context = LayoutContext::new(
        Some(ItemVariationStore::read(FontData::new(&store)).unwrap()),
        &axes,
    );
    let mut value = ValueRecord::new()
        .with_x_advance(100)
        .with_x_advance_device(VariationIndex::new(0, 0));
    value.apply(&ValuePass::Instance(&context)).unwrap();
    assert_eq!(value.x_advance, Some(110));
    assert!(value.x_advance_device.is_some());
    let mut removed = value.clone();
    let map = [(0, 0x0001_0002)].into_iter().collect();
    value.apply(&ValuePass::Remap(&map)).unwrap();
    assert_eq!(value.x_advance, Some(110));
    assert_eq!(
        value.x_advance_device.as_ref(),
        Some(&VariationIndex::new(1, 2).into())
    );
    removed
        .apply(&ValuePass::Remap(&optimize::IndexMap::new()))
        .unwrap();
    assert_eq!(removed.x_advance, Some(110));
    assert!(removed.x_advance_device.is_none());
}

#[test]
fn removed_variation_stores_and_rows_evaluate_to_zero() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
    let bytes = store_bytes(&[[10, 20]]);
    let store = ItemVariationStore::read(FontData::new(&bytes)).unwrap();
    for store in [None, Some(store)] {
        let c = LayoutContext {
            store,
            coords: &axes.coords,
            axes: &axes,
            condition_rows: RefCell::default(),
            scalars: OnceCell::new(),
            store_plan: OnceCell::new(),
            constant_indices: RefCell::default(),
        };
        assert_eq!(c.wide_delta(0, 1).unwrap(), 0);
        assert_eq!(c.wide_delta(1, 0).unwrap(), 0);
    }
}

#[test]
fn explicit_default_suppresses_neutral_layout_rows_in_partial_instances() {
    use write_fonts::tables::variations::*;
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let mut axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
    axes.coords.fill(F2Dot14::ZERO);
    // Keep one axis variable: HarfBuzz suppresses ordinary store deltas at
    // explicit all-zero coordinates independently of whether all axes are pinned.
    axes.pinned[0] = true;
    assert!(!axes.all_pinned());
    let neutral = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
    let bytes = write_fonts::dump_table(&write_fonts::tables::variations::ItemVariationStore {
        variation_region_list: VariationRegionList::new(
            axes.coords.len() as u16,
            vec![VariationRegion::new(vec![neutral; axes.coords.len()])],
        )
        .into(),
        item_variation_data: vec![ItemVariationData {
            item_count: 1,
            word_delta_count: 1,
            region_indexes: vec![0],
            delta_sets: 10i16.to_be_bytes().to_vec(),
        }
        .into()],
    })
    .unwrap();
    let c = LayoutContext::new(
        Some(
            read_fonts::tables::variations::ItemVariationStore::read(FontData::new(&bytes))
                .unwrap(),
        ),
        &axes,
    );
    assert_eq!(c.wide_delta(0, 0).unwrap(), 0);
    assert_eq!(c.condition_delta(0), 0.);
}

#[test]
fn condition_signs_and_positioning_deltas_use_distinct_precision() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(
        &font,
        &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
    )
    .unwrap();
    let bytes = store_bytes(&[
        [1, 0],
        [-1, 0],
        [i32::MAX, i32::MIN],
        [i32::MAX, 1 - i32::MAX],
    ]);
    for fraction in [0.25, 0.5, 1.] {
        let coords = vec![F2Dot14::from_f64(fraction); 2];
        let c = LayoutContext {
            store: Some(ItemVariationStore::read(FontData::new(&bytes)).unwrap()),
            coords: &coords,
            axes: &axes,
            condition_rows: RefCell::default(),
            scalars: OnceCell::new(),
            store_plan: OnceCell::new(),
            constant_indices: RefCell::default(),
        };
        assert!(condition(&Condition::format_2_variable_value(0, 0), &c, 0).unwrap());
        assert_eq!(
            condition(&Condition::format_2_variable_value(1, 2), &c, 0).unwrap(),
            fraction < 1.
        );
        assert!(condition(&Condition::format_2_variable_value(0, 3), &c, 0).unwrap());
        assert_eq!(
            c.wide_delta(0, 1).unwrap(),
            if fraction == 1. { -1 } else { 0 }
        );
        // Float positioning products lose the low bit of LONG_WORD values;
        // double condition products must retain it through cancellation.
        assert_eq!(c.wide_delta(0, 2).unwrap(), 0);
        assert_eq!(c.wide_delta(0, 3).unwrap(), 0);
        assert_eq!(c.condition_delta(2), -fraction);
        assert_eq!(c.condition_delta(3), fraction);
    }
}

#[test]
fn partially_pinned_constant_conditions_keep_the_unrounded_sign() {
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=525").unwrap()).unwrap();
    let bytes = store_bytes(&[[1, 0], [-1, 0]]);
    let c = LayoutContext {
        store: Some(ItemVariationStore::read(FontData::new(&bytes)).unwrap()),
        coords: &axes.coords,
        axes: &axes,
        condition_rows: RefCell::default(),
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        constant_indices: RefCell::default(),
    };
    assert_eq!(c.wide_delta(0, 0).unwrap(), 0);
    for (default, index, expected) in [
        (0, 0, true),
        (0, 1, false),
        (1, 1, true),
        (1, u32::MAX, true),
        (0, u32::MAX, false),
        (1, 100, true),
    ] {
        let PartialCondition::Constant(value) =
            partial_condition(&Condition::format_2_variable_value(default, index), &c, 0).unwrap()
        else {
            panic!()
        };
        assert_eq!(value, expected);
    }
}

#[test]
fn partial_instances_drop_constant_devices_and_keep_variable_and_hint_devices() {
    let source = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let source = FontRef::new(&source).unwrap();
    let mut builder = write_fonts::FontBuilder::new();
    for r in source.table_directory().table_records() {
        if r.tag() != Tag::new(b"avar") {
            builder.add_raw(r.tag(), source.data_for_tag(r.tag()).unwrap());
        }
    }
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    let raw = store_bytes(&[[3, 0], [-1, 0], [0, 5], [1, 5]]);
    let mut store: write_fonts::tables::variations::ItemVariationStore =
        ItemVariationStore::read(FontData::new(&raw))
            .unwrap()
            .to_owned_table();
    // Roboto's width axis varies below its default. Use a reachable tent.
    store.variation_region_list.variation_regions[1].region_axes[1] =
        write_fonts::tables::variations::RegionAxisCoordinates::new(
            F2Dot14::NEG_ONE,
            F2Dot14::NEG_ONE,
            F2Dot14::ZERO,
        );
    let store = write_fonts::dump_table(&store).unwrap();
    for (weight, gain) in [(525, 1), (650, 2), (900, 3)] {
        let axes = AxisPlan::new(
            &font,
            &crate::parse_axis_limits(&format!("wght={weight}")).unwrap(),
        )
        .unwrap();
        let c = LayoutContext {
            store: Some(ItemVariationStore::read(FontData::new(&store)).unwrap()),
            coords: &axes.coords,
            axes: &axes,
            condition_rows: RefCell::default(),
            scalars: OnceCell::new(),
            store_plan: OnceCell::new(),
            constant_indices: RefCell::default(),
        };
        let mut value = ValueRecord::new().with_x_advance_device(VariationIndex::new(0, 0));
        value.apply(&ValuePass::Instance(&c)).unwrap();
        assert_eq!(value.x_advance, Some(gain));
        assert_eq!(value.format(), ValueFormat::X_ADVANCE);
        assert!(value.x_advance_device.is_none());
        let hint = Device::new(10, 12, &[1, 0, -1]);
        let mut anchor = AnchorTable::Format3(AnchorFormat3 {
            x_coordinate: 100,
            y_coordinate: 200,
            x_device: Some(VariationIndex::new(0, 0).into()).into(),
            y_device: Some(hint.clone().into()).into(),
        });
        anchor.apply(&ValuePass::Instance(&c)).unwrap();
        let AnchorTable::Format3(anchor) = anchor else {
            panic!()
        };
        assert_eq!(anchor.x_coordinate, 100 + gain);
        assert!(anchor.x_device.is_none());
        assert_eq!(anchor.y_device.as_ref(), Some(&hint.clone().into()));
        let mut caret = CaretValue::format_3(100, VariationIndex::new(0, 1).into());
        caret.apply(&ValuePass::Instance(&c)).unwrap();
        assert_eq!(
            caret,
            CaretValue::format_1(if weight == 900 { 99 } else { 100 })
        );
        let mut base = BaseCoord::format_3(100, Some(VariationIndex::new(0, 0).into()));
        base.apply(&ValuePass::Instance(&c)).unwrap();
        assert_eq!(base, BaseCoord::format_3(100 + gain, None));
        for index in [2, 3] {
            let mut value = ValueRecord::new().with_x_advance_device(VariationIndex::new(0, index));
            value.apply(&ValuePass::Instance(&c)).unwrap();
            assert!(value.x_advance_device.is_some());
        }
        let mut value = ValueRecord::new().with_x_advance_device(hint.clone());
        value.apply(&ValuePass::Instance(&c)).unwrap();
        assert_eq!(value.x_advance_device.as_ref(), Some(&hint.into()));
    }

    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=100:400").unwrap()).unwrap();
    let c = LayoutContext {
        store: Some(ItemVariationStore::read(FontData::new(&store)).unwrap()),
        coords: &axes.coords,
        axes: &axes,
        condition_rows: RefCell::default(),
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        constant_indices: RefCell::default(),
    };
    assert!(c.index_is_constant(0).unwrap());
    assert!(!c.index_is_constant(2).unwrap());
}

#[test]
fn value_record_formats_merge_devices_and_new_base_fields_across_all_rows() {
    use read_fonts::tables::gpos::Gpos as ReadGpos;
    use read_fonts::types::GlyphId16;
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(
        &font,
        &crate::parse_axis_limits("wght=650,wdth=87.5").unwrap(),
    )
    .unwrap();
    let store = store_bytes(&[[1, 0], [-1, 0]]);
    let coords = [F2Dot14::from_f64(0.5); 2];
    let c = LayoutContext {
        store: Some(ItemVariationStore::read(FontData::new(&store)).unwrap()),
        coords: &coords,
        axes: &axes,
        condition_rows: RefCell::default(),
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        constant_indices: RefCell::default(),
    };
    for mixed in [false, true] {
        let empty = ValueRecord::new().with_explicit_value_format(ValueFormat::X_ADVANCE_DEVICE);
        let variable = ValueRecord::new()
            .with_x_advance_device(VariationIndex::new(0, if mixed { 0 } else { 1 }));
        let device = if mixed {
            ValueRecord::new().with_x_advance_device(Device::new(10, 12, &[1, 0, -1]))
        } else {
            empty.clone()
        };
        let values = vec![empty.clone(), variable, device];
        let cov = || CoverageTable::from_iter([1, 2, 3].map(GlyphId16::new));
        let single = SinglePos::format_2(cov(), values.clone());
        let pair1 = PairPos::format_1(
            CoverageTable::from_iter([GlyphId16::new(1)]),
            vec![PairSet::new(
                values
                    .iter()
                    .enumerate()
                    .map(|(i, value)| {
                        PairValueRecord::new(
                            GlyphId16::new(i as u16 + 1),
                            value.clone(),
                            ValueRecord::new(),
                        )
                    })
                    .collect(),
            )],
        );
        let pair2 = PairPos::format_2(
            CoverageTable::from_iter([GlyphId16::new(1)]),
            ClassDef::from_iter([(GlyphId16::new(1), 1)]),
            ClassDef::from_iter([(GlyphId16::new(1), 1), (GlyphId16::new(2), 2)]),
            vec![
                Class1Record::new(vec![Class2Record::new(empty, ValueRecord::new()); 3]),
                Class1Record::new(
                    values
                        .into_iter()
                        .map(|value| Class2Record::new(value, ValueRecord::new()))
                        .collect(),
                ),
            ],
        );
        let mut table = Gpos::new(
            ScriptList::default(),
            FeatureList::default(),
            LookupList::new(vec![
                PositionLookup::Single(Lookup::new(LookupFlag::empty(), vec![single.clone()])),
                PositionLookup::Pair(Lookup::new(
                    LookupFlag::empty(),
                    vec![pair1.clone(), pair2.clone()],
                )),
                PositionLookup::Extension(Lookup::new(
                    LookupFlag::empty(),
                    vec![ExtensionSubtable::Single(ExtensionPosFormat1::new(
                        1, single,
                    ))],
                )),
                PositionLookup::Extension(Lookup::new(
                    LookupFlag::empty(),
                    vec![
                        ExtensionSubtable::Pair(ExtensionPosFormat1::new(2, pair1)),
                        ExtensionSubtable::Pair(ExtensionPosFormat1::new(2, pair2)),
                    ],
                )),
            ]),
        );
        table.lookup_list.apply(&ValuePass::Instance(&c)).unwrap();
        let bytes = write_fonts::dump_table(&table).unwrap();
        let table = own_gpos(&ReadGpos::read(FontData::new(&bytes)).unwrap()).unwrap();
        let PositionLookup::Single(single) =
            table.lookup_list.as_ref().unwrap().lookups[0].as_ref()
        else {
            panic!()
        };
        let SinglePos::Format2(single) = single.subtables[0].as_ref() else {
            panic!()
        };
        assert_eq!(single.value_records.len(), 3);
        let fmt = if mixed {
            ValueFormat::X_ADVANCE | ValueFormat::X_ADVANCE_DEVICE
        } else {
            ValueFormat::empty()
        };
        for record in &single.value_records {
            assert_eq!(record.format(), fmt);
        }
        if mixed {
            assert_eq!(single.value_records[1].x_advance, Some(1));
            assert!(single.value_records[1].x_advance_device.is_none());
            assert!(matches!(
                single.value_records[2].x_advance_device.as_ref(),
                Some(DeviceOrVariationIndex::Device(_))
            ));
        }
        let PositionLookup::Pair(pair) = table.lookup_list.as_ref().unwrap().lookups[1].as_ref()
        else {
            panic!()
        };
        let PairPos::Format1(pair1) = pair.subtables[0].as_ref() else {
            panic!()
        };
        assert_eq!(pair1.pair_sets[0].pair_value_records.len(), 3);
        let PairPos::Format2(pair2) = pair.subtables[1].as_ref() else {
            panic!()
        };
        assert_eq!(pair2.class1_records.len(), 2);
        for row in &pair2.class1_records {
            assert_eq!(row.class2_records.len(), 3);
            for cell in &row.class2_records {
                assert_eq!(cell.value_record1.format(), fmt);
            }
        }
        // Extension wrappers must also retain zero-byte record counts.
        let PositionLookup::Extension(ext) =
            table.lookup_list.as_ref().unwrap().lookups[2].as_ref()
        else {
            panic!()
        };
        let ExtensionSubtable::Single(ext) = ext.subtables[0].as_ref() else {
            panic!()
        };
        let SinglePos::Format2(single) = ext.extension.as_ref() else {
            panic!()
        };
        assert_eq!(single.value_records.len(), 3);
        let PositionLookup::Extension(ext) =
            table.lookup_list.as_ref().unwrap().lookups[3].as_ref()
        else {
            panic!()
        };
        let ExtensionSubtable::Pair(ext) = ext.subtables[1].as_ref() else {
            panic!()
        };
        let PairPos::Format2(pair) = ext.extension.as_ref() else {
            panic!()
        };
        assert_eq!(pair.class1_records.len(), 2);
    }
}

#[test]
fn zero_byte_pair_matrices_survive_full_instancing() {
    use read_fonts::types::GlyphId16;
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(
        &font,
        &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
    )
    .unwrap();
    let pair = PairPos::format_2(
        CoverageTable::from_iter([GlyphId16::new(1), GlyphId16::new(2)]),
        ClassDef::from_iter([(GlyphId16::new(2), 2)]),
        ClassDef::from_iter([(GlyphId16::new(1), 4), (GlyphId16::new(2), 1)]),
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
    );
    let mut oversized = write_fonts::dump_table(&pair).unwrap();
    oversized[12..16].copy_from_slice(&[0xff; 4]);
    let borrowed = read_fonts::tables::gpos::PairPos::read(FontData::new(&oversized)).unwrap();
    let mut owned = borrowed.to_owned_table();
    assert!(own_empty_pairs(borrowed, &mut owned, &mut 200_000).is_err());
    for extension in [false, true] {
        let lookup = if extension {
            PositionLookup::Extension(Lookup::new(
                LookupFlag::empty(),
                vec![ExtensionSubtable::Pair(ExtensionPosFormat1::new(
                    2,
                    pair.clone(),
                ))],
            ))
        } else {
            PositionLookup::Pair(Lookup::new(LookupFlag::empty(), vec![pair.clone()]))
        };
        let gpos = Gpos::new(
            ScriptList::default(),
            FeatureList::default(),
            LookupList::new(vec![lookup]),
        );
        let mut builder = write_fonts::FontBuilder::new();
        for record in font.table_directory().table_records() {
            builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
        }
        builder.add_table(&gpos).unwrap();
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let tables = instance_layout(&font, &axes).unwrap();
        let output =
            read_fonts::tables::gpos::Gpos::read(FontData::new(tables.gpos.as_ref().unwrap()))
                .unwrap();
        let output = own_gpos(&output).unwrap();
        assert_eq!(output, gpos);
    }
}

#[test]
fn zero_byte_single_records_survive_direct_and_extension_instancing() {
    use read_fonts::types::GlyphId16;
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(
        &font,
        &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
    )
    .unwrap();
    let coverage = CoverageTable::from_iter([GlyphId16::new(1), GlyphId16::new(2)]);
    for single in [
        SinglePos::format_2(coverage.clone(), vec![ValueRecord::new(); 2]),
        SinglePos::format_4(coverage, vec![ValueRecord::new(); 2]),
    ] {
        for extension in [false, true] {
            let lookup = if extension {
                PositionLookup::Extension(Lookup::new(
                    LookupFlag::empty(),
                    vec![ExtensionSubtable::Single(ExtensionPosFormat1::new(
                        1,
                        single.clone(),
                    ))],
                ))
            } else {
                PositionLookup::Single(Lookup::new(LookupFlag::empty(), vec![single.clone()]))
            };
            let gpos = Gpos::new(
                ScriptList::default(),
                FeatureList::default(),
                LookupList::new(vec![lookup]),
            );
            let mut builder = write_fonts::FontBuilder::new();
            for record in font.table_directory().table_records() {
                builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
            }
            builder.add_table(&gpos).unwrap();
            let source = builder.build();
            let font = FontRef::new(&source).unwrap();
            let tables = instance_layout(&font, &axes).unwrap();
            let borrowed =
                read_fonts::tables::gpos::Gpos::read(FontData::new(tables.gpos.as_ref().unwrap()))
                    .unwrap();
            assert_eq!(own_gpos(&borrowed).unwrap(), gpos);
        }
    }
}

#[test]
fn long_word_deltas_saturate_fields_and_preserve_condition_sign() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(
        &font,
        &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
    )
    .unwrap();
    let bytes = store_bytes(&[[i32::MAX, i32::MAX], [i32::MIN, i32::MIN]]);
    let c = LayoutContext {
        store: Some(ItemVariationStore::read(FontData::new(&bytes)).unwrap()),
        coords: &axes.coords,
        axes: &axes,
        condition_rows: RefCell::default(),
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        constant_indices: RefCell::default(),
    };
    for (inner, base, expected) in [(0, 100, i16::MAX), (1, -100, i16::MIN)] {
        let mut value = ValueRecord::new()
            .with_x_advance(base)
            .with_x_advance_device(VariationIndex::new(0, inner));
        value.apply(&ValuePass::Instance(&c)).unwrap();
        assert_eq!(value.x_advance, Some(expected));
        assert!(value.x_advance_device.is_none());
        let cond = Condition::Format2VariableValue(ConditionFormat2 {
            default_value: base,
            var_index: inner as u32,
        });
        assert_eq!(condition(&cond, &c, 0).unwrap(), inner == 0);
    }
    let no_store = LayoutContext { store: None, ..c };
    let cond = Condition::Format2VariableValue(ConditionFormat2 {
        default_value: 1,
        var_index: u32::MAX,
    });
    assert!(condition(&cond, &no_store, 0).unwrap());
}

#[test]
fn partial_variable_conditions_preserve_large_folded_defaults() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
    let bytes = store_bytes(&[[i32::MAX, -i32::MAX]]);
    let c = LayoutContext {
        store: Some(ItemVariationStore::read(FontData::new(&bytes)).unwrap()),
        coords: &axes.coords,
        axes: &axes,
        condition_rows: RefCell::default(),
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        constant_indices: RefCell::default(),
    };
    let cond = Condition::Format2VariableValue(ConditionFormat2 {
        default_value: 100,
        var_index: 0,
    });
    let PartialCondition::Variable(Condition::Format2VariableValue(cond)) =
        partial_condition(&cond, &c, 0).unwrap()
    else {
        panic!()
    };
    assert_eq!(cond.default_value, i16::MAX);
    assert_eq!(cond.var_index, 1 << 16);
    let bytes = write_fonts::dump_table(&rebuild_store(&c).unwrap().unwrap()).unwrap();
    let store = ItemVariationStore::read(FontData::new(&bytes)).unwrap();
    for coord in [F2Dot14::ZERO, F2Dot14::from_f64(0.5), F2Dot14::ONE] {
        let delta = store
            .compute_delta(DeltaSetIndex { outer: 1, inner: 0 }, &[coord])
            .unwrap()
            .to_f64();
        // The folded default exceeds i32::MAX, so HarfBuzz scales this
        // Boolean expression by one half before storing its private row.
        assert_eq!(
            cond.default_value as f64 + delta,
            1073741874. - 1073741824. * coord.to_f64()
        );
        assert!(cond.default_value as f64 + delta > 0.);
    }
}

#[test]
fn scaled_condition_rows_preserve_thresholds_and_are_shared_by_matching_conditions() {
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=650").unwrap()).unwrap();
    let bytes = store_bytes(&[[1, -2]]);
    let original = ItemVariationStore::read(FontData::new(&bytes)).unwrap();
    let c = LayoutContext {
        store: Some(original.clone()),
        coords: &axes.coords,
        axes: &axes,
        condition_rows: RefCell::default(),
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        constant_indices: RefCell::default(),
    };
    let mut conditions = Vec::new();
    for default in [0, 1, 2, 0, 1, 2] {
        let condition = Condition::format_2_variable_value(default, 0);
        let PartialCondition::Variable(Condition::Format2VariableValue(condition)) =
            partial_condition(&condition, &c, 0).unwrap()
        else {
            panic!()
        };
        conditions.push((default, condition));
    }
    assert_eq!(c.condition_rows.borrow().rows.len(), 3);
    for i in 0..3 {
        assert_eq!(conditions[i].1, conditions[i + 3].1);
    }
    let rebuilt = write_fonts::dump_table(&rebuild_store(&c).unwrap().unwrap()).unwrap();
    let rebuilt = ItemVariationStore::read(FontData::new(&rebuilt)).unwrap();
    let gain = c.condition_delta(0);
    for (default, condition) in conditions {
        let threshold = F2Dot14::from_f64((default as f64 + gain) * 0.5).to_bits();
        for bits in [0, threshold - 1, threshold, threshold + 1, 16384] {
            let coord = F2Dot14::from_bits(bits.clamp(0, 16384));
            let expected = default as f64
                + VariationScalars::new(&original, &[axes.coords[0], coord])
                    .unwrap()
                    .condition_delta(&original, DeltaSetIndex { outer: 0, inner: 0 })
                    .unwrap();
            let actual = condition.default_value as f64
                + VariationScalars::new(&rebuilt, &[coord])
                    .unwrap()
                    .condition_delta(
                        &rebuilt,
                        DeltaSetIndex {
                            outer: (condition.var_index >> 16) as u16,
                            inner: condition.var_index as u16,
                        },
                    )
                    .unwrap();
            assert_eq!(
                actual > 0.,
                expected > 0.,
                "default={default} coord={coord}"
            );
        }
    }
}

#[test]
fn subsetting_partial_instances_preserves_variable_feature_conditions() {
    use read_fonts::{
        collections::IntSet,
        types::{GlyphId, GlyphId16, NameId},
    };
    let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let store_bytes = store_bytes(&[[i32::MAX, -i32::MAX]]);
    let store: write_fonts::tables::variations::ItemVariationStore =
        ItemVariationStore::read(FontData::new(&store_bytes))
            .unwrap()
            .to_owned_table();
    let gdef = Gdef {
        item_var_store: store.into(),
        ..Default::default()
    };
    let tree = Condition::format_5_negate(Condition::format_3_and(
        2,
        vec![
            Condition::format_2_variable_value(-100, 0),
            Condition::format_4_or(
                1,
                vec![Condition::format_1_axis_range(
                    1,
                    F2Dot14::ZERO,
                    F2Dot14::ONE,
                )],
            ),
        ],
    ));
    let mut gpos = Gpos::new(
        ScriptList::new(vec![ScriptRecord::new(
            Tag::new(b"DFLT"),
            Script::new(Some(LangSys::new(vec![0])), vec![]),
        )]),
        FeatureList::new(vec![FeatureRecord::new(
            Tag::new(b"kern"),
            Feature::new(None, vec![0]),
        )]),
        LookupList::new(
            [10, 20]
                .into_iter()
                .map(|advance| {
                    PositionLookup::Single(Lookup::new(
                        LookupFlag::empty(),
                        vec![SinglePos::Format1(SinglePosFormat1::new(
                            CoverageFormat1::new(vec![GlyphId16::new(1)]).into(),
                            ValueRecord::new().with_x_advance(advance),
                        ))],
                    ))
                })
                .collect(),
        ),
    );
    gpos.feature_variations = FeatureVariations {
        lookup_variation_records: None,
        feature_variation_records: vec![FeatureVariationRecord {
            condition_set: ConditionSet {
                conditions: vec![tree.into()],
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
    let mut builder = write_fonts::FontBuilder::new();
    for r in font.table_directory().table_records() {
        if ![Tag::new(b"GPOS"), Tag::new(b"GSUB"), Tag::new(b"GDEF")].contains(&r.tag()) {
            builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
        }
    }
    builder.add_table(&gpos).unwrap();
    builder.add_table(&gdef).unwrap();
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    let subset = |font: &FontRef| {
        let plan = crate::Plan::new(
            &[GlyphId::new(1)].into_iter().collect(),
            &IntSet::empty(),
            font,
            crate::SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::<NameId>::all(),
            &IntSet::all(),
        );
        crate::subset_font(font, &plan).unwrap()
    };
    // First preserve every recursive format during ordinary subsetting.
    let subset_bytes = subset(&font);
    let subset_font = FontRef::new(&subset_bytes).unwrap();
    let subset_gpos: Gpos = subset_font.gpos().unwrap().to_owned_table();
    assert_eq!(subset_gpos.feature_variations, gpos.feature_variations);
    // Then exercise the added GDEF bias row after partial instancing.
    let bytes =
        crate::instance_font(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
    let partial = FontRef::new(&bytes).unwrap();
    let bytes = subset(&partial);
    let partial_subset = FontRef::new(&bytes).unwrap();
    assert!(partial_subset.gdef().unwrap().item_var_store().is_some());
    for (contrast, expected) in [(0, 10), (100, 20)] {
        let bytes = crate::instance_font(
            &partial_subset,
            &crate::parse_axis_limits(&format!("CNTR={contrast}")).unwrap(),
        )
        .unwrap();
        let full = FontRef::new(&bytes).unwrap();
        let table: Gpos = full.gpos().unwrap().to_owned_table();
        let index = table.feature_list.as_ref().unwrap().feature_records[0]
            .feature
            .lookup_list_indices[0] as usize;
        let PositionLookup::Single(lookup) =
            table.lookup_list.as_ref().unwrap().lookups[index].as_ref()
        else {
            panic!()
        };
        let SinglePos::Format1(pos) = lookup.subtables[0].as_ref() else {
            panic!()
        };
        assert_eq!(pos.value_record.x_advance, Some(expected));
    }
}

#[test]
fn deep_conditions_are_rejected_before_owned_conversion() {
    let mut bytes = [0, 5, 0, 0, 5].repeat(4096);
    bytes.extend([0, 1, 0, 0, 0, 0, 0x40, 0]);
    assert!(crate::conditions::own(FontData::new(&bytes), 33, &mut 200_000).is_err());
}

#[test]
fn partial_feature_conditions_keep_priority_and_remap_axes() {
    let data = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let font = FontRef::new(&data).unwrap();
    let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
    let c = LayoutContext {
        store: None,
        coords: &axes.coords,
        axes: &axes,
        condition_rows: RefCell::default(),
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        constant_indices: RefCell::default(),
    };
    let range = |axis| {
        Condition::Format1AxisRange(ConditionFormat1 {
            axis_index: axis,
            filter_range_min_value: F2Dot14::from_f64(0.5),
            filter_range_max_value: F2Dot14::ONE,
        })
    };
    let record = |conditions: Vec<Condition>, index| FeatureVariationRecord {
        condition_set: ConditionSet {
            conditions: conditions.into_iter().map(Into::into).collect(),
        }
        .into(),
        feature_table_substitution: FeatureTableSubstitution {
            substitutions: vec![FeatureTableSubstitutionRecord {
                feature_index: 0,
                alternate_feature: Feature {
                    lookup_list_indices: vec![index],
                    ..Default::default()
                }
                .into(),
            }],
        }
        .into(),
    };
    let mut vars: NullableOffsetMarker<FeatureVariations, 4> = FeatureVariations {
        lookup_variation_records: None,
        feature_variation_records: vec![
            record(vec![range(0), range(1)], 1),
            record(vec![range(0)], 2),
        ],
    }
    .into();
    let mut list = FeatureList {
        feature_records: vec![FeatureRecord {
            feature_tag: Tag::new(b"rvrn"),
            feature: Feature {
                lookup_list_indices: vec![0],
                ..Default::default()
            }
            .into(),
        }],
    };
    instance_features(&mut list, &mut vars, &c).unwrap();
    assert_eq!(vars.as_ref().unwrap().feature_variation_records.len(), 2);
    let set = vars.as_ref().unwrap().feature_variation_records[0]
        .condition_set
        .as_ref()
        .unwrap();
    let Condition::Format1AxisRange(cond) = set.conditions[0].as_ref() else {
        panic!()
    };
    assert_eq!(cond.axis_index, 0);
    for (value, expected) in [(0., 2), (75., 1)] {
        let axes = AxisPlan::new(
            &font,
            &crate::parse_axis_limits(&format!("wght=900,CNTR={value}")).unwrap(),
        )
        .unwrap();
        // The retained condition's index is in the output's axis order.
        let coords = vec![axes.coords[1]];
        let c = LayoutContext {
            store: None,
            coords: &coords,
            axes: &axes,
            condition_rows: RefCell::default(),
            scalars: OnceCell::new(),
            store_plan: OnceCell::new(),
            constant_indices: RefCell::default(),
        };
        let mut list = list.clone();
        let mut vars = vars.clone();
        instance_features(&mut list, &mut vars, &c).unwrap();
        assert_eq!(
            list.feature_records[0].feature.lookup_list_indices,
            vec![expected]
        );
        assert!(vars.is_none());
    }
}

#[test]
fn feature_variation_headers_are_checked_before_owning() {
    let features = write_fonts::dump_table(&FeatureList::default()).unwrap();
    let features = read_fonts::tables::layout::FeatureList::read(FontData::new(&features)).unwrap();
    for (major, minor, count) in [(1u16, 0u16, u32::MAX), (2, 0, 0)] {
        let bytes = major
            .to_be_bytes()
            .into_iter()
            .chain(minor.to_be_bytes())
            .chain(count.to_be_bytes())
            .collect::<Vec<_>>();
        let vars =
            read_fonts::tables::layout::FeatureVariations::read(FontData::new(&bytes)).unwrap();
        assert!(own_feature_variations(Some(vars), &features, Tag::new(b"GSUB")).is_err());
    }
    let empty = [0, 1, 0, 0, 0, 0, 0, 0];
    let vars = read_fonts::tables::layout::FeatureVariations::read(FontData::new(&empty)).unwrap();
    assert!(own_feature_variations(Some(vars), &features, Tag::new(b"GSUB")).is_ok());
}
