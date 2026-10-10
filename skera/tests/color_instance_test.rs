//! Paint values use HarfBuzz's float accumulation and rounding.
//! References were generated with HarfBuzz c82300aefb, all glyphs, retained
//! IDs, and the full and partial requests in the test below.
use skera::{instance_font, parse_axis_limits};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{FontData, FontRead, FontRef, TableProvider},
    tables::{colr::*, variations::*},
    types::{F2Dot14, FWord, Fixed, GlyphId16, Tag, UfWord},
    FontBuilder,
};

fn source() -> Vec<u8> {
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let palette = FontRef::new(font_test_data::COLRV0V1_VARIABLE).unwrap();
    let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
    let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
    let neg = RegionAxisCoordinates::new(F2Dot14::NEG_ONE, F2Dot14::NEG_ONE, F2Dot14::ZERO);
    let rows = [
        [-1, 0],
        [-16777217, 16777216],
        [1, 0],
        [0, -1],
        [0, 0],
        [1, 0],
        [-1, 0],
        [0, -1],
        [-1, 0],
        [0, -1],
        [-1, 0],
        [0, -1],
    ];
    let store = ItemVariationStore::new(
        VariationRegionList::new(
            2,
            vec![
                VariationRegion::new(vec![pos, zero.clone()]),
                VariationRegion::new(vec![zero, neg]),
            ],
        ),
        vec![Some(ItemVariationData {
            item_count: rows.len() as u16,
            word_delta_count: 0x8002,
            region_indexes: vec![0, 1],
            delta_sets: rows
                .into_iter()
                .flatten()
                .flat_map(i32::to_be_bytes)
                .collect(),
        })],
    );
    let line = || {
        VarColorLine::new(
            Extend::Pad,
            2,
            vec![
                VarColorStop::new(F2Dot14::ZERO, 0, F2Dot14::ONE, 6),
                VarColorStop::new(F2Dot14::ONE, 0, F2Dot14::ONE, 8),
            ],
        )
    };
    let paints = [
        Paint::VarTransform(PaintVarTransform::new(
            Paint::VarTranslate(PaintVarTranslate::new(
                Paint::VarSolid(PaintVarSolid::new(0, F2Dot14::ONE, 6)),
                FWord::new(100),
                FWord::new(200),
                6,
            )),
            VarAffine2x3::new(
                Fixed::ONE,
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::ONE,
                Fixed::from_i32(100),
                Fixed::from_i32(200),
                0,
            ),
        )),
        Paint::VarRadialGradient(PaintVarRadialGradient::new(
            line(),
            FWord::new(1),
            FWord::new(2),
            UfWord::new(100),
            FWord::new(3),
            FWord::new(4),
            UfWord::new(200),
            6,
        )),
        Paint::VarSweepGradient(PaintVarSweepGradient::new(
            line(),
            FWord::new(100),
            FWord::new(200),
            F2Dot14::NEG_ONE,
            F2Dot14::ONE,
            6,
        )),
    ];
    let colr = Colr {
        base_glyph_list: Some(BaseGlyphList::new(
            3,
            paints
                .into_iter()
                .enumerate()
                .map(|(i, paint)| BaseGlyphPaint::new(GlyphId16::new(i as u16 + 1), paint))
                .collect(),
        ))
        .into(),
        clip_list: Some(ClipList::new(
            1,
            1,
            vec![Clip::new(
                GlyphId16::new(1),
                GlyphId16::new(3),
                ClipBox::Format2(ClipBoxFormat2::new(
                    FWord::new(0),
                    FWord::new(0),
                    FWord::new(500),
                    FWord::new(700),
                    6,
                )),
            )],
        ))
        .into(),
        item_variation_store: Some(store).into(),
        ..Default::default()
    };
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
    }
    builder.add_raw(
        Tag::new(b"CPAL"),
        palette.data_for_tag(Tag::new(b"CPAL")).unwrap(),
    );
    builder.add_table(&colr).unwrap();
    builder.build()
}

#[test]
fn half_deltas_and_large_cancellation_match_harfbuzz_paints() {
    let source = source();
    for (weight, width) in [(525, 87.5), (650, 75.), (900, 75.)] {
        let bytes = instance_font(
            &FontRef::new(&source).unwrap(),
            &parse_axis_limits(&format!("wght={weight},wdth={width}")).unwrap(),
        )
        .unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let expected = std::fs::read(format!(
            "test-data/expected/color-instance/{weight}-COLR.bin"
        ))
        .unwrap();
        let expected: Colr = write_fonts::read::tables::colr::Colr::read(FontData::new(&expected))
            .unwrap()
            .to_owned_table();
        let actual: Colr = font.colr().unwrap().to_owned_table();
        assert_eq!(actual, expected, "weight={weight}");
    }
    let partial = instance_font(
        &FontRef::new(&source).unwrap(),
        &parse_axis_limits("wght=650").unwrap(),
    )
    .unwrap();
    let bytes = instance_font(
        &FontRef::new(&partial).unwrap(),
        &parse_axis_limits("wdth=75").unwrap(),
    )
    .unwrap();
    let expected = std::fs::read("test-data/expected/color-instance/composed-COLR.bin").unwrap();
    let expected: Colr = write_fonts::read::tables::colr::Colr::read(FontData::new(&expected))
        .unwrap()
        .to_owned_table();
    let actual: Colr = FontRef::new(&bytes)
        .unwrap()
        .colr()
        .unwrap()
        .to_owned_table();
    assert_eq!(actual, expected);
}
