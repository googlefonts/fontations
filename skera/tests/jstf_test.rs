//! Justification data shares layout glyph mappings and GDEF variation rows.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, tables::variations::DeltaSetIndex, FontRef, TableProvider},
    tables::{gpos::*, jstf::*, layout::DeviceOrVariationIndex},
    types::{F2Dot14, GlyphId, Tag},
};

const SOURCE: &[u8] = include_bytes!("../test-data/fonts/jstf-variations.ttf");
const LOOKUPS: &[u8] = include_bytes!("../test-data/fonts/jstf-lookup-closure.ttf");

fn instance(bytes: &[u8], request: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
    .unwrap()
}

fn priorities(table: &Jstf) -> Vec<&JstfPriority> {
    let script = table.jstf_script_records[0].jstf_script.as_ref();
    [
        script.def_jstf_lang_sys.as_ref().unwrap(),
        script.jstf_lang_sys_records[0].jstf_lang_sys.as_ref(),
    ]
    .into_iter()
    .flat_map(|language| {
        language
            .jstf_priorities
            .iter()
            .map(|priority| priority.as_ref())
    })
    .collect()
}

fn check(bytes: &[u8], weight: f32, width: f32) {
    let font = FontRef::new(bytes).unwrap();
    let table: Jstf = font.jstf().unwrap().to_owned_table();
    let coords: Vec<_> = font
        .fvar()
        .ok()
        .map(|fvar| {
            fvar.axes()
                .unwrap()
                .iter()
                .map(|axis| {
                    F2Dot14::from_f32(if axis.axis_tag() == Tag::new(b"wght") {
                        weight
                    } else {
                        width
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let store = font
        .gdef()
        .ok()
        .and_then(|gdef| gdef.item_var_store())
        .transpose()
        .unwrap();
    let value = |base: i16, device: Option<&DeviceOrVariationIndex>| {
        let mut value = f64::from(base);
        if let Some(DeviceOrVariationIndex::VariationIndex(index)) = device {
            value += store
                .as_ref()
                .unwrap()
                .compute_delta(
                    DeltaSetIndex {
                        outer: index.delta_set_outer_index,
                        inner: index.delta_set_inner_index,
                    },
                    &coords,
                )
                .unwrap()
                .to_f64();
        }
        if coords.is_empty() {
            assert!(device.is_none());
        }
        value
    };
    let reference: Jstf = FontRef::new(SOURCE)
        .unwrap()
        .jstf()
        .unwrap()
        .to_owned_table();
    for (i, priority) in priorities(&table).into_iter().enumerate() {
        for maximum in [
            priority.shrinkage_jstf_max.as_ref().unwrap(),
            priority.extension_jstf_max.as_ref().unwrap(),
        ] {
            assert_eq!(maximum.lookups.len(), 3);
            let PositionLookup::Single(single) = maximum.lookups[0].as_ref() else {
                panic!()
            };
            let SinglePos::Format1(single) = single.subtables[0].as_ref() else {
                panic!()
            };
            let record = &single.value_record;
            assert_eq!(
                value(
                    record.x_placement.unwrap_or(0),
                    record.x_placement_device.as_ref()
                ),
                f64::from(10. + (i % 2) as f32 * 3. + weight * 10. + width * 20.)
            );
            assert_eq!(
                value(
                    record.y_placement.unwrap_or(0),
                    record.y_placement_device.as_ref()
                ),
                f64::from(20. - weight * 20. + width * 30.)
            );
            assert_eq!(record.x_advance, Some(30));
            let PositionLookup::Cursive(cursive) = maximum.lookups[1].as_ref() else {
                panic!()
            };
            let CursivePos::Format1(cursive) = cursive.subtables[0].as_ref() else {
                panic!()
            };
            let anchors = &cursive.entry_exit_record[0];
            let anchor = anchors.entry_anchor.as_ref().unwrap();
            let (x, y, dx, dy) = match anchor {
                AnchorTable::Format1(v) => (v.x_coordinate, v.y_coordinate, None, None),
                AnchorTable::Format3(v) => (
                    v.x_coordinate,
                    v.y_coordinate,
                    v.x_device.as_ref(),
                    v.y_device.as_ref(),
                ),
                _ => panic!(),
            };
            assert_eq!(value(x, dx), f64::from(100. + weight * 30. - width * 10.));
            assert_eq!(value(y, dy), f64::from(200. + weight * 40. - width * 20.));
            let AnchorTable::Format3(exit) = anchors.exit_anchor.as_ref().unwrap() else {
                panic!("hinting Device must survive")
            };
            let ref_max = priorities(&reference)[i]
                .shrinkage_jstf_max
                .as_ref()
                .unwrap();
            let PositionLookup::Cursive(ref_cursive) = ref_max.lookups[1].as_ref() else {
                panic!()
            };
            let CursivePos::Format1(ref_cursive) = ref_cursive.subtables[0].as_ref() else {
                panic!()
            };
            let AnchorTable::Format3(ref_exit) = ref_cursive.entry_exit_record[0]
                .exit_anchor
                .as_ref()
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(exit, ref_exit);
        }
    }
}

#[test]
fn full_and_partial_jstf_instances_preserve_values_and_devices() {
    for weight in [0., 0.5, 1.] {
        for width in [0., 0.5, 1.] {
            let direct = instance(SOURCE, &format!("wght={weight},wdth={width}"));
            check(&direct, weight, width);
            for (first, second) in [
                (format!("wght={weight}"), format!("wdth={width}")),
                (format!("wdth={width}"), format!("wght={weight}")),
            ] {
                let partial = instance(SOURCE, &first);
                check(&partial, weight, width);
                let composed = instance(&partial, &second);
                check(&composed, weight, width);
                let direct = FontRef::new(&direct).unwrap();
                let composed = FontRef::new(&composed).unwrap();
                assert_eq!(
                    direct.data_for_tag(Tag::new(b"JSTF")).unwrap().as_bytes(),
                    composed.data_for_tag(Tag::new(b"JSTF")).unwrap().as_bytes()
                );
            }
        }
    }
}

#[test]
fn zero_byte_value_records_keep_their_coverage_and_count() {
    let bytes = instance(SOURCE, "wght=1,wdth=0");
    let table: Jstf = FontRef::new(&bytes)
        .unwrap()
        .jstf()
        .unwrap()
        .to_owned_table();
    for priority in priorities(&table) {
        for maximum in [
            priority.shrinkage_jstf_max.as_ref().unwrap(),
            priority.extension_jstf_max.as_ref().unwrap(),
        ] {
            let PositionLookup::Single(lookup) = maximum.lookups[2].as_ref() else {
                panic!()
            };
            let SinglePos::Format2(single) = lookup.subtables[0].as_ref() else {
                panic!()
            };
            assert_eq!(single.coverage.iter().count(), 2);
            // The borrowed array cannot recover zero-byte records; use its count field.
            let read = FontRef::new(&bytes).unwrap().jstf().unwrap();
            let script = read.jstf_script_records()[0]
                .jstf_script(read.offset_data())
                .unwrap();
            let lang = script.def_jstf_lang_sys().unwrap().unwrap();
            let priority = lang.jstf_priorities().get(0).unwrap();
            let maximum = priority.shrinkage_jstf_max().unwrap().unwrap();
            let write_fonts::read::tables::gpos::PositionLookup::Single(lookup) =
                maximum.lookups().get(2).unwrap()
            else {
                panic!()
            };
            let write_fonts::read::tables::gpos::SinglePos::Format2(single) =
                lookup.subtables().get(0).unwrap()
            else {
                panic!()
            };
            assert_eq!(single.value_count(), 2);
            assert!(single.value_format().is_empty());
        }
    }
}

#[test]
fn jstf_only_variation_rows_survive_subsetting_and_later_instancing() {
    let source = FontRef::new(SOURCE).unwrap();
    let bytes = subset_font(&source, &Plan::keep_everything(&source)).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let store = font.gdef().unwrap().item_var_store().unwrap().unwrap();
    assert_eq!(
        store
            .item_variation_data()
            .get(0)
            .unwrap()
            .unwrap()
            .item_count(),
        4
    );
    for weight in [0., 0.5, 1.] {
        let partial = instance(&bytes, &format!("wght={weight}"));
        let font = FontRef::new(&partial).unwrap();
        let partial = subset_font(&font, &Plan::keep_everything(&font)).unwrap();
        for width in [0., 0.5, 1.] {
            check(&partial, weight, width);
            check(&instance(&partial, &format!("wdth={width}")), weight, width);
        }
    }
}

#[test]
fn enabled_lookups_and_extenders_participate_in_glyph_closure() {
    let font = FontRef::new(LOOKUPS).unwrap();
    let plan = Plan::new(
        &[GlyphId::new(2)].into_iter().collect(),
        &IntSet::empty(),
        &font,
        SubsetFlags::default(),
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
    );
    let mut mapping = plan.old_to_new_glyph_mapping().collect::<Vec<_>>();
    mapping.sort_unstable();
    assert_eq!(
        mapping,
        vec![
            (GlyphId::new(0), GlyphId::new(0)),
            (GlyphId::new(2), GlyphId::new(1)),
            (GlyphId::new(3), GlyphId::new(2)),
            (GlyphId::new(4), GlyphId::new(3))
        ]
    );
    let bytes = subset_font(&font, &plan).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    assert_eq!(
        font.gsub().unwrap().lookup_list().unwrap().lookup_count(),
        1
    );
    assert_eq!(
        font.gpos().unwrap().lookup_list().unwrap().lookup_count(),
        1
    );
    let table: Jstf = font.jstf().unwrap().to_owned_table();
    assert_eq!(
        table.jstf_script_records[0]
            .jstf_script
            .extender_glyph
            .as_ref()
            .unwrap()
            .extender_glyphs
            .iter()
            .map(|g| g.to_u32())
            .collect::<Vec<_>>(),
        vec![3]
    );
    for priority in priorities(&table) {
        assert_eq!(
            priority
                .gsub_shrinkage_enable
                .as_ref()
                .unwrap()
                .lookup_indices,
            vec![0]
        );
        assert_eq!(
            priority
                .gsub_extension_disable
                .as_ref()
                .unwrap()
                .lookup_indices,
            vec![0]
        );
        assert_eq!(
            priority
                .gpos_shrinkage_enable
                .as_ref()
                .unwrap()
                .lookup_indices,
            vec![0]
        );
        assert!(priority.gpos_shrinkage_disable.is_none());
    }
    check(&instance(&bytes, "wght=1,wdth=0"), 1., 0.);
}

#[test]
fn malformed_jstf_is_rejected_and_explicit_dropping_skips_it() {
    let source = FontRef::new(SOURCE).unwrap();
    let tag = Tag::new(b"JSTF");
    let original = source.data_for_tag(tag).unwrap().as_bytes();
    for mutation in 0..3 {
        let mut jstf = original.to_vec();
        match mutation {
            0 => jstf.truncate(6),
            1 => jstf[10..12].copy_from_slice(&u16::MAX.to_be_bytes()),
            2 => jstf[..4].copy_from_slice(&[0, 2, 0, 0]),
            _ => unreachable!(),
        }
        let mut builder = write_fonts::FontBuilder::new();
        for record in source.table_directory().table_records() {
            builder.add_raw(record.tag(), source.data_for_tag(record.tag()).unwrap());
        }
        builder.add_raw(tag, &jstf);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        assert!(
            matches!(instance_font(&font, &parse_axis_limits("wght=1,wdth=0").unwrap()), Err(skera::SubsetError::SubsetTableError(t)) if t == tag)
        );
        assert!(
            matches!(subset_font(&font, &Plan::keep_everything(&font)), Err(skera::SubsetError::SubsetTableError(t)) if t == tag)
        );
        let plan = Plan::new(
            &IntSet::all(),
            &IntSet::empty(),
            &font,
            SubsetFlags::default(),
            &[tag].into_iter().collect(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
        );
        let dropped = subset_font(&font, &plan).unwrap();
        assert!(FontRef::new(&dropped).unwrap().data_for_tag(tag).is_none());
    }
}
