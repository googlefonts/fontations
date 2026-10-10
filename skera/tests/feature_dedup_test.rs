//! Same-tag features are duplicates only when their complete behavior matches.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{
        collections::IntSet,
        tables::layout::{FeatureList, FeatureParams, FeatureVariations},
        types::NameId,
        types::Tag,
        FontData, FontRead, FontRef, TableProvider,
    },
};

const SOURCE: &[u8] = include_bytes!("../test-data/fonts/layout-feature-dedup.ttf");

fn subset(bytes: &[u8]) -> Vec<u8> {
    let font = FontRef::new(bytes).unwrap();
    let plan = Plan::new(
        &IntSet::all(),
        &IntSet::empty(),
        &font,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::from_iter((0..=6).map(NameId::new)),
        &IntSet::all(),
    );
    subset_font(&font, &plan).unwrap()
}

fn check(list: &FeatureList, variations: Option<FeatureVariations>, varying: bool) {
    let tagged = |tag: &[u8; 4]| {
        list.feature_records()
            .iter()
            .filter(|r| r.feature_tag() == Tag::new(tag))
            .map(|r| r.feature(list.offset_data()).unwrap())
            .collect::<Vec<_>>()
    };
    let cv = tagged(b"cv01");
    assert_eq!(cv.len(), 2);
    for (feature, character) in cv.iter().zip([0x1f600, 0x1f601]) {
        let FeatureParams::CharacterVariant(params) = feature.feature_params().unwrap().unwrap()
        else {
            panic!()
        };
        assert_eq!(params.character()[0].get().to_u32(), character);
    }
    let size = tagged(b"size");
    assert_eq!(size.len(), 2);
    for (feature, design_size) in size.iter().zip([1000, 1100]) {
        let FeatureParams::Size(params) = feature.feature_params().unwrap().unwrap() else {
            panic!()
        };
        assert_eq!(params.design_size(), design_size);
    }
    let ss = tagged(b"ss01");
    assert_eq!(ss.len(), 2);
    for (feature, name) in ss.iter().zip([300, 400]) {
        let FeatureParams::StylisticSet(params) = feature.feature_params().unwrap().unwrap() else {
            panic!()
        };
        assert_eq!(params.ui_name_id(), NameId::new(name));
    }
    assert_eq!(tagged(b"ss02").len(), if varying { 2 } else { 1 });
    // Genuine duplicates still merge.
    assert_eq!(tagged(b"ss03").len(), 1);
    if let Some(variations) = variations {
        let substitutions = variations.feature_variation_records()[0]
            .feature_table_substitution(variations.offset_data())
            .unwrap()
            .unwrap();
        assert_eq!(substitutions.substitution_count(), 1);
        let alternate = &substitutions.substitutions()[0];
        let feature_index = alternate.feature_index() as usize;
        assert_eq!(
            list.feature_records()[feature_index].feature_tag(),
            Tag::new(b"ss02")
        );
        assert_eq!(
            alternate
                .alternate_feature(substitutions.offset_data())
                .unwrap()
                .lookup_list_indices()[0]
                .get(),
            1
        );
    }
}

#[test]
fn parameters_and_alternates_preserve_language_specific_features() {
    for limits in [
        None,
        Some("wdth=87.5"),
        Some("wght=650,wdth=87.5"),
        Some("wght=400,wdth=87.5"),
    ] {
        let instance = limits.map(|limits| {
            instance_font(
                &FontRef::new(SOURCE).unwrap(),
                &parse_axis_limits(limits).unwrap(),
            )
            .unwrap()
        });
        let output = subset(instance.as_deref().unwrap_or(SOURCE));
        let font = FontRef::new(&output).unwrap();
        let gsub = font.gsub().unwrap();
        let gpos = font.gpos().unwrap();
        for (features, variations) in [
            (gsub.feature_list().unwrap(), gsub.feature_variations()),
            (gpos.feature_list().unwrap(), gpos.feature_variations()),
        ] {
            check(
                &features,
                variations.transpose().unwrap(),
                limits != Some("wght=400,wdth=87.5"),
            );
        }
        let names = font.name().unwrap();
        for id in [300, 301, 302, 303, 400] {
            assert!(names
                .name_record()
                .iter()
                .any(|r| r.name_id() == NameId::new(id)));
        }
    }
}

#[test]
fn layout_structures_match_harfbuzz() {
    for (case, limits) in [
        ("subset", None),
        ("partial", Some("wdth=87.5")),
        ("full-active", Some("wght=650,wdth=87.5")),
        ("full-default", Some("wght=400,wdth=87.5")),
    ] {
        let instance = limits.map(|limits| {
            instance_font(
                &FontRef::new(SOURCE).unwrap(),
                &parse_axis_limits(limits).unwrap(),
            )
            .unwrap()
        });
        let output = subset(instance.as_deref().unwrap_or(SOURCE));
        let font = FontRef::new(&output).unwrap();
        let bytes =
            std::fs::read(format!("test-data/expected/feature-dedup/{case}-GSUB.bin")).unwrap();
        let expected: write_fonts::tables::gsub::Gsub =
            write_fonts::read::tables::gsub::Gsub::read(FontData::new(&bytes))
                .unwrap()
                .to_owned_table();
        let actual: write_fonts::tables::gsub::Gsub = font.gsub().unwrap().to_owned_table();
        assert_eq!(actual, expected, "{case} GSUB");
        let bytes =
            std::fs::read(format!("test-data/expected/feature-dedup/{case}-GPOS.bin")).unwrap();
        let expected: write_fonts::tables::gpos::Gpos =
            write_fonts::read::tables::gpos::Gpos::read(FontData::new(&bytes))
                .unwrap()
                .to_owned_table();
        let actual: write_fonts::tables::gpos::Gpos = font.gpos().unwrap().to_owned_table();
        assert_eq!(actual, expected, "{case} GPOS");
    }
}
