//! Compact mapped COLR rows while preserving implicit consecutive indices.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, types::NameId, FontData, FontRead, FontRef, TableProvider},
    tables::{colr::*, variations::*},
    types::{F2Dot14, FWord, GlyphId16, Tag},
    FontBuilder,
};

fn source(mapped: bool, empty_map: bool, constant_only: bool) -> Vec<u8> {
    let bytes = include_bytes!("../test-data/fonts/Roboto-Variable.composite.ttf");
    let font = FontRef::new(bytes).unwrap();
    let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
    let positive = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
    let negative = RegionAxisCoordinates::new(F2Dot14::NEG_ONE, F2Dot14::NEG_ONE, F2Dot14::ZERO);
    let data = |rows: &[[i16; 2]]| ItemVariationData {
        item_count: rows.len() as u16,
        word_delta_count: 2,
        region_indexes: vec![0, 1],
        delta_sets: rows
            .iter()
            .flat_map(|r| {
                [r[0], if constant_only { 0 } else { r[1] }]
                    .into_iter()
                    .flat_map(i16::to_be_bytes)
            })
            .collect(),
    };
    let store = ItemVariationStore::new(
        VariationRegionList::new(
            2,
            vec![
                VariationRegion::new(vec![positive, zero.clone()]),
                VariationRegion::new(vec![zero, negative]),
            ],
        ),
        vec![
            Some(data(&[[100, 1000], [100, 1000], [100, 0], [100, -2000]])),
            Some(data(&[[-100, -2000], [100, 1000]])),
        ],
    );
    let indices = [0u32, 1, 2, 3, 0x10000, 0x10001];
    let map = mapped.then(|| {
        if empty_map {
            DeltaSetIndexMap::from_iter([] as [u32; 0])
        } else {
            indices.into_iter().collect()
        }
    });
    let explicit = mapped && !empty_map;
    let paints = [0, 2, if explicit { 4 } else { 0x10000 }]
        .into_iter()
        .enumerate()
        .map(|(i, base)| {
            BaseGlyphPaint::new(
                GlyphId16::new(i as u16 + 1),
                Paint::VarTranslate(PaintVarTranslate::new(
                    Paint::VarSolid(PaintVarSolid::new(
                        0,
                        F2Dot14::from_f32(0.5),
                        if explicit { 100 } else { 0 },
                    )),
                    FWord::new(20),
                    FWord::new(30),
                    base,
                )),
            )
        })
        .collect();
    let colr = Colr {
        base_glyph_list: Some(BaseGlyphList::new(3, paints)).into(),
        var_index_map: map.into(),
        item_variation_store: Some(store).into(),
        ..Default::default()
    };
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
    }
    let palette = FontRef::new(font_test_data::COLRV0V1_VARIABLE).unwrap();
    builder.add_raw(
        Tag::new(b"CPAL"),
        palette.data_for_tag(Tag::new(b"CPAL")).unwrap(),
    );
    builder.add_table(&colr).unwrap();
    builder.build()
}

fn instance(source: &[u8], limits: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(source).unwrap(),
        &parse_axis_limits(limits).unwrap(),
    )
    .unwrap()
}

fn check_composition(source: &[u8], partial: &[u8]) {
    let font = FontRef::new(partial).unwrap();
    let plan = Plan::new(
        &IntSet::all(),
        &IntSet::empty(),
        &font,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::<NameId>::all(),
        &IntSet::all(),
    );
    let subset = subset_font(&font, &plan).unwrap();
    assert_eq!(
        FontRef::new(&subset)
            .unwrap()
            .colr()
            .unwrap()
            .base_glyph_list()
            .unwrap()
            .unwrap()
            .num_base_glyph_paint_records(),
        3
    );
    for width in [75., 87.5, 100.] {
        let direct = instance(source, &format!("wght=650,wdth={width}"));
        let composed = instance(partial, &format!("wdth={width}"));
        let direct: Colr = FontRef::new(&direct)
            .unwrap()
            .colr()
            .unwrap()
            .to_owned_table();
        let composed: Colr = FontRef::new(&composed)
            .unwrap()
            .colr()
            .unwrap()
            .to_owned_table();
        assert_eq!(direct, composed, "width={width}");
        let subset = instance(&subset, &format!("wdth={width}"));
        let subset: Colr = FontRef::new(&subset)
            .unwrap()
            .colr()
            .unwrap()
            .to_owned_table();
        assert_eq!(direct, subset, "subset width={width}");
    }
}

#[test]
fn mapped_rows_deduplicate_zero_rows_and_repeat_last_indices() {
    let source = source(true, false, false);
    let partial = instance(&source, "wght=650");
    let font = FontRef::new(&partial).unwrap();
    let colr = font.colr().unwrap();
    let store = colr.item_variation_store().unwrap().unwrap();
    let count: u32 = store
        .item_variation_data()
        .iter()
        .filter_map(|r| r.and_then(Result::ok))
        .map(|r| u32::from(r.item_count()))
        .sum();
    assert_eq!(count, 2);
    let map = colr.var_index_map().unwrap().unwrap();
    assert_eq!(map.get(0).unwrap(), map.get(1).unwrap());
    assert_eq!(map.get(0).unwrap(), map.get(5).unwrap());
    assert_eq!(map.get(3).unwrap(), map.get(4).unwrap());
    assert_eq!(
        map.get(2).unwrap(),
        write_fonts::read::tables::variations::DeltaSetIndex::NO_VARIATION_INDEX
    );
    assert_eq!(map.get(100).unwrap(), map.get(5).unwrap());
    check_composition(&source, &partial);
}

#[test]
fn implicit_and_empty_maps_preserve_consecutive_fields() {
    for empty_map in [false, true] {
        let source = source(empty_map, empty_map, false);
        let partial = instance(&source, "wght=650");
        let font = FontRef::new(&partial).unwrap();
        let colr = font.colr().unwrap();
        let store = colr.item_variation_store().unwrap().unwrap();
        if empty_map {
            let map = colr.var_index_map().unwrap().unwrap();
            assert_eq!(
                map.get(0x10001).unwrap(),
                write_fonts::read::tables::variations::DeltaSetIndex { outer: 1, inner: 1 }
            );
        } else {
            assert!(colr.var_index_map().is_none());
        }
        assert_eq!(store.item_variation_data_count(), 2);
        assert_eq!(
            store
                .item_variation_data()
                .get(0)
                .unwrap()
                .unwrap()
                .item_count(),
            4
        );
        assert_eq!(
            store
                .item_variation_data()
                .get(1)
                .unwrap()
                .unwrap()
                .item_count(),
            2
        );
        check_composition(&source, &partial);
    }
}

#[test]
fn an_empty_mapped_store_becomes_no_variation_indices() {
    let source = source(true, false, true);
    let partial = instance(&source, "wght=650");
    let font = FontRef::new(&partial).unwrap();
    let colr = font.colr().unwrap();
    assert!(colr.item_variation_store().is_none());
    let map = colr.var_index_map().unwrap().unwrap();
    for index in [0, 1, 2, 3, 4, 5, 100] {
        assert_eq!(
            map.get(index).unwrap(),
            write_fonts::read::tables::variations::DeltaSetIndex::NO_VARIATION_INDEX
        );
    }
    check_composition(&source, &partial);
}

#[test]
fn surviving_regions_precede_newly_rebased_regions() {
    use write_fonts::read::tables::variations::VariationRegionList as ReadRegions;
    for (request, expected) in [
        (
            "ROTA=134.997:269.995:404.992",
            include_bytes!("../test-data/expected/color-store/ROTA-regions.bin").as_slice(),
        ),
        (
            "APH1=-0.75:-0.5:-0.25",
            include_bytes!("../test-data/expected/color-store/APH1-regions.bin").as_slice(),
        ),
        (
            "APH2=-0.75:-0.5:-0.25",
            include_bytes!("../test-data/expected/color-store/APH2-regions.bin").as_slice(),
        ),
    ] {
        let partial = instance(font_test_data::COLRV0V1_VARIABLE, request);
        let font = FontRef::new(&partial).unwrap();
        let colr = font.colr().unwrap();
        let store = colr.item_variation_store().unwrap().unwrap();
        let expected = ReadRegions::read(FontData::new(expected)).unwrap();
        let expected: VariationRegionList = expected.to_owned_table();
        let actual: VariationRegionList = store.variation_region_list().unwrap().to_owned_table();
        assert_eq!(actual, expected, "{request}");
    }
}

#[test]
fn residual_tables_match_harfbuzz() {
    for (kind, mapped, constant_only) in [
        ("mapped", true, false),
        ("implicit", false, false),
        ("constant", true, true),
    ] {
        let source = source(mapped, false, constant_only);
        let partial = instance(&source, "wght=650");
        let font = FontRef::new(&partial).unwrap();
        let plan = Plan::new(
            &IntSet::all(),
            &IntSet::empty(),
            &font,
            SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
        );
        let subset = subset_font(&font, &plan).unwrap();
        let actual: Colr = FontRef::new(&subset)
            .unwrap()
            .colr()
            .unwrap()
            .to_owned_table();
        let reference =
            std::fs::read(format!("test-data/expected/color-store/{kind}-COLR.bin")).unwrap();
        let expected: Colr = write_fonts::read::tables::colr::Colr::read(FontData::new(&reference))
            .unwrap()
            .to_owned_table();
        assert_eq!(actual, expected, "{kind}");
    }
}
