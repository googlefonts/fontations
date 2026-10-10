//! Alternate features use the parameter format of their original feature tag.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontData, FontRead, FontRef, ResolveOffset, TableProvider},
    tables::{gpos::*, gsub::*, layout::*, name::*},
    types::{F2Dot14, GlyphId16, NameId, Tag, Uint24},
    FontBuilder,
};

fn params(tag: Tag, first: u16) -> FeatureParams {
    match &tag.to_be_bytes() {
        b"cv01" => FeatureParams::CharacterVariant(CharacterVariantParams::new(
            NameId::new(first),
            NameId::new(first + 1),
            NameId::new(first + 2),
            3,
            NameId::new(first + 3),
            vec![Uint24::new(0x41), Uint24::new(0x1f600)],
        )),
        b"size" => FeatureParams::Size(SizeParams::new(120, 1, first, 100, 140)),
        b"ss01" => FeatureParams::StylisticSet(StylisticSetParams::new(NameId::new(first))),
        _ => panic!(),
    }
}

fn synthetic_font() -> Vec<u8> {
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let source = FontRef::new(&bytes).unwrap();
    let tags = [Tag::new(b"cv01"), Tag::new(b"size"), Tag::new(b"ss01")];
    let mut records = vec![FeatureRecord::new(
        Tag::new(b"aalt"),
        Feature::new(None, vec![0]),
    )];
    records.extend(
        tags.map(|tag| FeatureRecord::new(tag, Feature::new(Some(params(tag, 300)), vec![0]))),
    );
    let features = FeatureList::new(records);
    let scripts = ScriptList::new(vec![ScriptRecord::new(
        Tag::new(b"DFLT"),
        Script::new(Some(LangSys::new(vec![0, 1, 2, 3])), vec![]),
    )]);
    let variations = FeatureVariations {
        feature_variation_records: vec![FeatureVariationRecord {
            condition_set: ConditionSet::new(vec![Condition::format_1_axis_range(
                0,
                F2Dot14::from_f32(0.25),
                F2Dot14::ONE,
            )])
            .into(),
            feature_table_substitution: FeatureTableSubstitution {
                substitutions: tags
                    .into_iter()
                    .enumerate()
                    .map(|(i, tag)| FeatureTableSubstitutionRecord {
                        feature_index: i as u16 + 1,
                        alternate_feature: Feature::new(Some(params(tag, 400)), vec![1]).into(),
                    })
                    .collect(),
            }
            .into(),
        }],
    };
    let coverage = CoverageTable::from_iter([GlyphId16::new(1)]);
    let mut gsub = Gsub::new(
        scripts.clone(),
        features.clone(),
        SubstitutionLookupList::new(
            [2, 3]
                .into_iter()
                .map(|target| {
                    SubstitutionLookup::Single(Lookup::new(
                        LookupFlag::empty(),
                        vec![SingleSubst::format_2(
                            coverage.clone(),
                            vec![GlyphId16::new(target)],
                        )],
                    ))
                })
                .collect(),
        ),
    );
    gsub.feature_variations = variations.clone().into();
    let mut gpos = Gpos::new(
        scripts,
        features,
        LookupList::new(
            [10, 20]
                .into_iter()
                .map(|advance| {
                    PositionLookup::Single(Lookup::new(
                        LookupFlag::empty(),
                        vec![SinglePosFormat1::new(
                            coverage.clone(),
                            ValueRecord::new().with_x_advance(advance),
                        )
                        .into()],
                    ))
                })
                .collect(),
        ),
    );
    gpos.feature_variations = variations.into();
    let mut names: Name = source.name().unwrap().to_owned_table();
    for first in [300, 400] {
        for id in first..first + 6 {
            names.name_record.push(NameRecord::new(
                3,
                1,
                0x409,
                NameId::new(id),
                format!("Feature label {id}").into(),
            ));
        }
    }
    names.name_record.sort();
    let mut builder = FontBuilder::new();
    for record in source.table_directory().table_records() {
        if record.tag() != Tag::new(b"avar") {
            builder.add_raw(record.tag(), source.data_for_tag(record.tag()).unwrap());
        }
    }
    builder.add_table(&gsub).unwrap();
    builder.add_table(&gpos).unwrap();
    builder.add_table(&names).unwrap();
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
        &IntSet::from_iter([Tag::new(b"cv01"), Tag::new(b"size"), Tag::new(b"ss01")]),
        &IntSet::from_iter((0..=6).map(NameId::new)),
        &IntSet::all(),
    );
    subset_font(&font, &plan).unwrap()
}

fn check(bytes: &[u8], first: u16, has_variations: bool) {
    let font = FontRef::new(bytes).unwrap();
    let gsub = font.gsub().unwrap();
    let gpos = font.gpos().unwrap();
    for (list, variations) in [
        (gsub.feature_list().unwrap(), gsub.feature_variations()),
        (gpos.feature_list().unwrap(), gpos.feature_variations()),
    ] {
        assert_eq!(list.feature_count(), 3);
        for record in list.feature_records() {
            let feature = record.feature(list.offset_data()).unwrap();
            let actual: FeatureParams = feature.feature_params().unwrap().unwrap().to_owned_table();
            assert_eq!(actual, params(record.feature_tag(), first),);
        }
        assert_eq!(variations.is_some(), has_variations);
        if let Some(variations) = variations {
            let variations = variations.unwrap();
            for record in variations.feature_variation_records() {
                let substitutions = record
                    .feature_table_substitution(variations.offset_data())
                    .unwrap()
                    .unwrap();
                assert_eq!(substitutions.substitution_count(), 3);
                for substitution in substitutions.substitutions() {
                    let tag =
                        list.feature_records()[substitution.feature_index() as usize].feature_tag();
                    let alternate = substitution
                        .alternate_feature_offset()
                        .resolve_with_args::<write_fonts::read::tables::layout::Feature>(
                            substitutions.offset_data(),
                            tag,
                        )
                        .unwrap();
                    let actual: FeatureParams = alternate
                        .feature_params()
                        .unwrap()
                        .unwrap()
                        .to_owned_table();
                    assert_eq!(actual, params(tag, 400));
                }
            }
        }
    }
    let names = font.name().unwrap();
    let ids = IntSet::from_iter(names.name_record().iter().map(|r| r.name_id()));
    for first in [first, if has_variations { 400 } else { first }] {
        for id in first..first + 6 {
            assert!(ids.contains(NameId::new(id)), "missing name {id}");
        }
    }
}

#[test]
fn alternate_parameters_and_names_survive_subsetting_and_instancing() {
    let source = synthetic_font();
    check(&subset(&source), 300, true);
    for (limits, first, has_variations) in [
        ("wght=400,wdth=87.5", 300, false),
        ("wght=650,wdth=87.5", 400, false),
        ("wdth=87.5", 300, true),
        ("wght=100:400:650", 300, true),
        ("wght=650", 300, true),
    ] {
        let instance = instance_font(
            &FontRef::new(&source).unwrap(),
            &parse_axis_limits(limits).unwrap(),
        )
        .unwrap();
        check(&subset(&instance), first, has_variations);
    }
    let partial = instance_font(
        &FontRef::new(&source).unwrap(),
        &parse_axis_limits("wdth=87.5").unwrap(),
    )
    .unwrap();
    let full = instance_font(
        &FontRef::new(&subset(&partial)).unwrap(),
        &parse_axis_limits("wght=650").unwrap(),
    )
    .unwrap();
    check(&subset(&full), 400, false);
}

fn own_variations(
    source: Option<write_fonts::read::tables::layout::FeatureVariations>,
    features: &write_fonts::read::tables::layout::FeatureList,
) -> Option<FeatureVariations> {
    let source = source?;
    let mut result: FeatureVariations = source.to_owned_table();
    for (record, owned) in source
        .feature_variation_records()
        .iter()
        .zip(&mut result.feature_variation_records)
    {
        let substitutions = record
            .feature_table_substitution(source.offset_data())
            .unwrap()
            .unwrap();
        for (record, owned) in substitutions.substitutions().iter().zip(
            &mut owned
                .feature_table_substitution
                .as_mut()
                .unwrap()
                .substitutions,
        ) {
            let tag = features.feature_records()[record.feature_index() as usize].feature_tag();
            let alternate = record
                .alternate_feature_offset()
                .resolve_with_args::<write_fonts::read::tables::layout::Feature>(
                    substitutions.offset_data(),
                    tag,
                )
                .unwrap();
            let alternate: Feature = alternate.to_owned_table();
            owned.alternate_feature = alternate.into();
        }
    }
    Some(result)
}

#[test]
fn layout_structures_match_harfbuzz() {
    // HarfBuzz c82300aefb, all glyphs, retained IDs, notdef outline, and
    // --layout-features=cv01,size,ss01. References contain layout tables only;
    // HarfBuzz drops alternate parameters and their names. Their preservation
    // is checked independently above; compare the remaining layout structure.
    let source = synthetic_font();
    for (case, limits) in [
        ("subset", None),
        ("full-default", Some("wght=400,wdth=87.5")),
        ("full-active", Some("wght=650,wdth=87.5")),
        ("partial", Some("wdth=87.5")),
    ] {
        let bytes = if let Some(limits) = limits {
            instance_font(
                &FontRef::new(&source).unwrap(),
                &parse_axis_limits(limits).unwrap(),
            )
            .unwrap()
        } else {
            source.clone()
        };
        let bytes = subset(&bytes);
        let font = FontRef::new(&bytes).unwrap();
        let gsub = font.gsub().unwrap();
        let mut actual: Gsub = gsub.to_owned_table();
        actual.feature_variations = own_variations(
            gsub.feature_variations().transpose().unwrap(),
            &gsub.feature_list().unwrap(),
        )
        .into();
        clear_alternate_params(actual.feature_variations.as_mut());
        let expected =
            std::fs::read(format!("test-data/expected/feature-params/{case}-GSUB.bin")).unwrap();
        let expected =
            write_fonts::read::tables::gsub::Gsub::read(FontData::new(&expected)).unwrap();
        let mut owned: Gsub = expected.to_owned_table();
        owned.feature_variations = own_variations(
            expected.feature_variations().transpose().unwrap(),
            &expected.feature_list().unwrap(),
        )
        .into();
        assert_eq!(actual, owned, "{case} GSUB");

        let gpos = font.gpos().unwrap();
        let mut actual: Gpos = gpos.to_owned_table();
        actual.feature_variations = own_variations(
            gpos.feature_variations().transpose().unwrap(),
            &gpos.feature_list().unwrap(),
        )
        .into();
        clear_alternate_params(actual.feature_variations.as_mut());
        let expected =
            std::fs::read(format!("test-data/expected/feature-params/{case}-GPOS.bin")).unwrap();
        let expected =
            write_fonts::read::tables::gpos::Gpos::read(FontData::new(&expected)).unwrap();
        let mut owned: Gpos = expected.to_owned_table();
        owned.feature_variations = own_variations(
            expected.feature_variations().transpose().unwrap(),
            &expected.feature_list().unwrap(),
        )
        .into();
        assert_eq!(actual, owned, "{case} GPOS");
    }
}

fn clear_alternate_params(variations: Option<&mut FeatureVariations>) {
    if let Some(variations) = variations {
        for record in &mut variations.feature_variation_records {
            for substitution in &mut record
                .feature_table_substitution
                .as_mut()
                .unwrap()
                .substitutions
            {
                substitution.alternate_feature.feature_params = Default::default();
            }
        }
    }
}
