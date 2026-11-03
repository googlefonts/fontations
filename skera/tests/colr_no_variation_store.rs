use read_fonts::{
    types::{F2Dot14, GlyphId16, Tag},
    FontRef, TableProvider,
};
use skera::{instance_font, parse_axis_limits};
use write_fonts::{tables::colr::*, FontBuilder};

#[test]
fn radial_sentinel_mapping_without_store_can_partially_instance() {
    let source = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let font = FontRef::new(&source).unwrap();
    let colr = Colr {
        base_glyph_list: Some(BaseGlyphList::new(
            1,
            vec![BaseGlyphPaint::new(
                GlyphId16::new(1),
                Paint::VarRadialGradient(PaintVarRadialGradient::new(
                    VarColorLine::new(
                        Extend::Pad,
                        2,
                        vec![
                            VarColorStop::new(F2Dot14::ZERO, 0, F2Dot14::ONE, 6),
                            VarColorStop::new(F2Dot14::ONE, 1, F2Dot14::ONE, 8),
                        ],
                    ),
                    0.into(),
                    0.into(),
                    10.into(),
                    100.into(),
                    0.into(),
                    20.into(),
                    0,
                )),
            )],
        ))
        .into(),
        var_index_map: Some(
            [u32::MAX; 10]
                .into_iter()
                .collect::<write_fonts::tables::variations::DeltaSetIndexMap>(),
        )
        .into(),
        ..Default::default()
    };
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        if record.tag() != Tag::new(b"COLR") {
            builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
        }
    }
    builder.add_table(&colr).unwrap();
    builder
        .add_table(&write_fonts::tables::cpal::Cpal::new(
            2,
            1,
            2,
            Some(vec![
                write_fonts::tables::cpal::ColorRecord::new(0, 0, 0, 255),
                write_fonts::tables::cpal::ColorRecord::new(255, 255, 255, 255),
            ]),
            vec![0],
        ))
        .unwrap();
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    let colr = font.colr().unwrap();
    assert!(colr.item_variation_store().is_none());
    assert_eq!(
        colr.var_index_map().unwrap().unwrap().get(0).unwrap(),
        read_fonts::tables::variations::DeltaSetIndex::NO_VARIATION_INDEX
    );
    let result = instance_font(&font, &parse_axis_limits("wght=650").unwrap());
    let result = result.unwrap();
    let result = FontRef::new(&result).unwrap();
    let colr = result.colr().unwrap();
    let owned: write_fonts::tables::colr::Colr =
        write_fonts::from_obj::ToOwnedTable::to_owned_table(&colr);
    let paint = &owned
        .base_glyph_list
        .as_ref()
        .unwrap()
        .base_glyph_paint_records[0]
        .paint;
    let Paint::VarRadialGradient(radial) = &**paint else {
        panic!("expected radial gradient");
    };
    assert_eq!(radial.radius0.to_u16(), 10);
    assert_eq!(radial.radius1.to_u16(), 20);
    assert_eq!(radial.color_line.color_stops[0].stop_offset, F2Dot14::ZERO);
    assert_eq!(radial.color_line.color_stops[1].stop_offset, F2Dot14::ONE);
}
