//! Null condition offsets are true; format zero is invalid.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontData, FontRead, FontRef, TableProvider},
    tables::{gpos::*, gsub::*, layout::*},
    types::{F2Dot14, GlyphId16, Tag},
    FontBuilder,
};

fn coverage() -> CoverageTable {
    CoverageTable::from_iter([GlyphId16::new(1)])
}

fn condition_bytes(kind: u8) -> Option<Vec<u8>> {
    match kind {
        0 => None,
        3 | 4 => {
            // One null child and one range on the weight axis.
            let mut bytes = vec![0, kind, 2, 0, 0, 0, 0, 0, 9];
            bytes.extend([0, 1, 0, 0, 0, 0, 0x40, 0]);
            Some(bytes)
        }
        5 => Some(vec![0, 5, 0, 0, 0]),
        _ => Some(vec![0, 0]),
    }
}

fn synthetic_font(tag: Tag, kind: u8) -> Vec<u8> {
    let source = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
    let source = FontRef::new(&source).unwrap();
    let scripts = ScriptList::new(vec![ScriptRecord::new(
        Tag::new(b"DFLT"),
        Script::new(Some(LangSys::new(vec![0])), vec![]),
    )]);
    let features = FeatureList::new(vec![FeatureRecord::new(
        Tag::new(b"liga"),
        Feature::new(None, vec![0]),
    )]);
    let vars = FeatureVariations {
        feature_variation_records: vec![
            FeatureVariationRecord {
                condition_set: ConditionSet {
                    conditions: vec![Condition::format_1_axis_range(
                        0,
                        F2Dot14::ZERO,
                        F2Dot14::ONE,
                    )
                    .into()],
                }
                .into(),
                feature_table_substitution: FeatureTableSubstitution {
                    substitutions: vec![FeatureTableSubstitutionRecord {
                        feature_index: 0,
                        alternate_feature: Feature::new(None, vec![1]).into(),
                    }],
                }
                .into(),
            },
            FeatureVariationRecord {
                // A later unconditional record must not override a true first one.
                condition_set: ConditionSet { conditions: vec![] }.into(),
                feature_table_substitution: FeatureTableSubstitution {
                    substitutions: vec![FeatureTableSubstitutionRecord {
                        feature_index: 0,
                        alternate_feature: Feature::new(None, vec![0]).into(),
                    }],
                }
                .into(),
            },
        ],
    };
    let mut raw = if tag == Tag::new(b"GSUB") {
        let mut table = Gsub::new(
            scripts,
            features,
            SubstitutionLookupList::new(
                [2, 3]
                    .into_iter()
                    .map(|gid| {
                        SubstitutionLookup::Single(Lookup::new(
                            LookupFlag::empty(),
                            vec![
                                SingleSubstFormat2::new(coverage(), vec![GlyphId16::new(gid)])
                                    .into(),
                            ],
                        ))
                    })
                    .collect(),
            ),
        );
        table.feature_variations = vars.into();
        write_fonts::dump_table(&table).unwrap()
    } else {
        let mut table = Gpos::new(
            scripts,
            features,
            LookupList::new(
                [10, 20]
                    .into_iter()
                    .map(|advance| {
                        PositionLookup::Single(Lookup::new(
                            LookupFlag::empty(),
                            vec![SinglePosFormat1::new(
                                coverage(),
                                ValueRecord::new().with_x_advance(advance),
                            )
                            .into()],
                        ))
                    })
                    .collect(),
            ),
        );
        table.feature_variations = vars.into();
        write_fonts::dump_table(&table).unwrap()
    };
    let variations = if tag == Tag::new(b"GSUB") {
        write_fonts::read::tables::gsub::Gsub::read(FontData::new(&raw))
            .unwrap()
            .feature_variations()
    } else {
        write_fonts::read::tables::gpos::Gpos::read(FontData::new(&raw))
            .unwrap()
            .feature_variations()
    }
    .unwrap()
    .unwrap();
    let set = variations.feature_variation_records()[0]
        .condition_set(variations.offset_data())
        .unwrap()
        .unwrap();
    let base = set.offset_data().as_bytes().as_ptr() as usize - raw.as_ptr() as usize;
    let offset = if let Some(tree) = condition_bytes(kind) {
        let offset = (raw.len() - base) as u32;
        raw.extend(tree);
        offset
    } else {
        0
    };
    raw[base + 2..base + 6].copy_from_slice(&offset.to_be_bytes());
    let mut builder = FontBuilder::new();
    for record in source.table_directory().table_records() {
        if ![Tag::new(b"GSUB"), Tag::new(b"GPOS"), Tag::new(b"GDEF")].contains(&record.tag()) {
            builder.add_raw(record.tag(), source.data_for_tag(record.tag()).unwrap());
        }
    }
    builder.add_raw(tag, raw);
    builder.build()
}

fn subset(bytes: &[u8]) -> Vec<u8> {
    let font = FontRef::new(bytes).unwrap();
    let plan = Plan::new(
        &skera::populate_gids("1").unwrap(),
        &IntSet::empty(),
        &font,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
    );
    subset_font(&font, &plan).unwrap()
}

fn assert_selected(bytes: &[u8], tag: Tag, alternate: bool) {
    let font = FontRef::new(bytes).unwrap();
    let expected_index = u16::from(alternate);
    if tag == Tag::new(b"GSUB") {
        let table: Gsub = font.gsub().unwrap().to_owned_table();
        assert!(table.feature_variations.is_none());
        assert_eq!(
            table.feature_list.feature_records[0]
                .feature
                .lookup_list_indices,
            [expected_index]
        );
    } else {
        let table: Gpos = font.gpos().unwrap().to_owned_table();
        assert!(table.feature_variations.is_none());
        assert_eq!(
            table.feature_list.feature_records[0]
                .feature
                .lookup_list_indices,
            [expected_index]
        );
    }
}

#[test]
fn null_conditions_preserve_feature_selection_through_subsetting_and_instancing() {
    for tag in [Tag::new(b"GSUB"), Tag::new(b"GPOS")] {
        for kind in [0, 3, 4, 5] {
            let bytes = synthetic_font(tag, kind);
            let subset_bytes = subset(&bytes);
            for source in [&bytes, &subset_bytes] {
                for weight in [200, 900] {
                    let alternate = kind == 0 || kind == 4 || (kind == 3 && weight == 900);
                    let font = FontRef::new(source).unwrap();
                    let full = instance_font(
                        &font,
                        &parse_axis_limits(&format!("wght={weight},CNTR=0")).unwrap(),
                    )
                    .unwrap();
                    assert_selected(&full, tag, alternate);
                    // Preserve the condition while pinning the other axis; then
                    // subset and finish instancing at each side of its range.
                    let partial =
                        instance_font(&font, &parse_axis_limits("CNTR=0").unwrap()).unwrap();
                    let partial = subset(&partial);
                    let full = instance_font(
                        &FontRef::new(&partial).unwrap(),
                        &parse_axis_limits(&format!("wght={weight}")).unwrap(),
                    )
                    .unwrap();
                    // Subsetting can prune a lookup after a constant condition
                    // is eliminated; check its action rather than its old index.
                    let full = FontRef::new(&full).unwrap();
                    if tag == Tag::new(b"GSUB") {
                        let table: Gsub = full.gsub().unwrap().to_owned_table();
                        let i = table.feature_list.feature_records[0]
                            .feature
                            .lookup_list_indices[0] as usize;
                        let SubstitutionLookup::Single(lookup) =
                            table.lookup_list.lookups[i].as_ref()
                        else {
                            panic!()
                        };
                        let substitute = match lookup.subtables[0].as_ref() {
                            SingleSubst::Format1(sub) => {
                                GlyphId16::new(1u16.wrapping_add(sub.delta_glyph_id as u16))
                            }
                            SingleSubst::Format2(sub) => sub.substitute_glyph_ids[0],
                        };
                        assert_eq!(substitute, GlyphId16::new(if alternate { 3 } else { 2 }));
                    } else {
                        let table: Gpos = full.gpos().unwrap().to_owned_table();
                        let i = table.feature_list.feature_records[0]
                            .feature
                            .lookup_list_indices[0] as usize;
                        let PositionLookup::Single(lookup) = table.lookup_list.lookups[i].as_ref()
                        else {
                            panic!()
                        };
                        let SinglePos::Format1(pos) = lookup.subtables[0].as_ref() else {
                            panic!()
                        };
                        assert_eq!(
                            pos.value_record.x_advance,
                            Some(if alternate { 20 } else { 10 })
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn format_zero_is_rejected_by_subsetting_and_instancing() {
    for tag in [Tag::new(b"GSUB"), Tag::new(b"GPOS")] {
        let bytes = synthetic_font(tag, 255);
        let font = FontRef::new(&bytes).unwrap();
        let plan = Plan::new(
            &skera::populate_gids("1").unwrap(),
            &IntSet::empty(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
        );
        assert!(
            matches!(subset_font(&font, &plan), Err(skera::SubsetError::SubsetTableError(t)) if t == tag)
        );
        for limits in ["CNTR=0", "wght=900,CNTR=0"] {
            assert!(
                matches!(instance_font(&font, &parse_axis_limits(limits).unwrap()), Err(skera::SubsetError::SubsetTableError(t)) if t == tag)
            );
        }
    }
}
