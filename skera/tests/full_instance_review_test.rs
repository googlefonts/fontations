//! Regressions found while reviewing the full CFF2 instancing slice.
use read_fonts::{
    model::glyph::outline::PathElement,
    ps::cff::CffFontRef,
    types::{GlyphId, Tag},
    FontRef, TableProvider,
};
use skera::{instance_font, parse_axis_limits, AxisLimits, SubsetError};

#[test]
fn full_blends_match_harfbuzz_rounding_and_scalar_precision() {
    let bytes = include_bytes!("../test-data/fonts/cff2_fractional_blends.otf");
    let font = FontRef::new(bytes).unwrap();
    // HarfBuzz 14.6.0 resolves the complete blend operand, rather than its
    // delta alone. Its float 1/3 scalar also puts B just above half a unit.
    for (location, starts) in [(0., [1., 0.]), (0.5, [1., 0.]), (1. / 16384., [1., 1.])] {
        let output = instance_font(
            &font,
            &[AxisLimits::Pin {
                tag: Tag::new(b"wght"),
                value: location,
            }],
        )
        .unwrap();
        let output = FontRef::new(&output).unwrap();
        let data = output.data_for_tag(Tag::new(b"CFF2")).unwrap();
        let cff = CffFontRef::new(data.as_bytes(), 0, None).unwrap();
        for (index, start) in starts.into_iter().enumerate() {
            let gid = GlyphId::new(index as u32 + 1);
            let subfont = cff.subfont(cff.subfont_index(gid).unwrap(), &[]).unwrap();
            let mut path = Vec::<PathElement>::new();
            cff.draw(&subfont, gid, &[], None, &mut path).unwrap();
            assert_eq!(path[0], PathElement::MoveTo { x: start, y: 0. });
            assert_eq!(output.hmtx().unwrap().side_bearing(gid), Some(start as i16));
        }
    }
}

#[test]
fn pinning_outside_an_axis_clamps_all_consumers() {
    let bytes = include_bytes!("../test-data/fonts/AdobeVFPrototype.otf");
    let font = FontRef::new(bytes).unwrap();
    for (outside, endpoint) in [(1100., 900.), (0., 200.)] {
        let make = |weight| {
            instance_font(
                &font,
                &[
                    AxisLimits::Pin {
                        tag: Tag::new(b"wght"),
                        value: weight,
                    },
                    AxisLimits::Drop {
                        tag: Tag::new(b"CNTR"),
                    },
                ],
            )
            .unwrap()
        };
        assert_eq!(make(outside), make(endpoint));
    }
}

#[test]
fn invalid_api_ranges_return_errors() {
    let bytes = include_bytes!("../test-data/fonts/Cantarell-VF-ABC.otf");
    let font = FontRef::new(bytes).unwrap();
    for (min, max) in [
        (f32::NAN, f32::NAN),
        (f32::NAN, 800.),
        (100., f32::NAN),
        (f32::NEG_INFINITY, f32::INFINITY),
        (800., 100.),
    ] {
        assert!(matches!(
            instance_font(
                &font,
                &[AxisLimits::Range {
                    tag: Tag::new(b"wght"),
                    min,
                    default: None,
                    max,
                }]
            ),
            Err(SubsetError::InvalidAxis(_))
        ));
    }
}

#[test]
fn variable_color_is_resolved_before_removing_axes() {
    use read_fonts::tables::colr::Paint;
    let font = FontRef::new(include_bytes!("../test-data/fonts/cff2_variable_color.otf")).unwrap();
    let bytes = instance_font(&font, &parse_axis_limits("wght=0.5").unwrap()).unwrap();
    let output = FontRef::new(&bytes).unwrap();
    assert!(output.fvar().is_err());
    let colr = output.colr().unwrap();
    assert!(colr.item_variation_store().is_none());
    let (Paint::Glyph(glyph), _) = colr.v1_base_glyph(GlyphId::new(1)).unwrap() else {
        panic!("expected a glyph paint");
    };
    let Paint::Solid(solid) = glyph.paint().unwrap() else {
        panic!("expected resolved static paint");
    };
    assert_eq!(solid.alpha().to_f32(), 0.75);
}

#[test]
fn normalization_quantizes_before_mapping_and_blending() {
    let font = FontRef::new(include_bytes!("../test-data/fonts/Cantarell-VF-ABC.otf")).unwrap();
    let output = instance_font(&font, &parse_axis_limits("wght=746.1").unwrap()).unwrap();
    let font = FontRef::new(&output).unwrap();
    let data = font.data_for_tag(Tag::new(b"CFF2")).unwrap();
    let cff = CffFontRef::new(data.as_bytes(), 0, None).unwrap();
    let gid = GlyphId::new(3);
    let subfont = cff.subfont(cff.subfont_index(gid).unwrap(), &[]).unwrap();
    let mut path = Vec::<PathElement>::new();
    cff.draw(&subfont, gid, &[], None, &mut path).unwrap();
    // HarfBuzz 14.6.0: normalizing through Fixed first changes this y to 660.
    assert!(path.contains(&PathElement::LineTo { x: 614., y: 659. }));
    assert!(!path.contains(&PathElement::LineTo { x: 614., y: 660. }));
}

#[test]
fn partial_instances_preserve_untouched_axis_records() {
    let font = FontRef::new(include_bytes!("../test-data/fonts/AdobeVFPrototype.otf")).unwrap();
    let original = font.fvar().unwrap();
    let original = &original.axes().unwrap()[0];
    assert_ne!(
        original.default_value().to_f64(),
        original.default_value().to_f32() as f64
    );
    for request in ["CNTR=13.7", "CNTR=25:50:75"] {
        let bytes = instance_font(&font, &parse_axis_limits(request).unwrap()).unwrap();
        let output = FontRef::new(&bytes).unwrap();
        let fvar = output.fvar().unwrap();
        let retained = &fvar.axes().unwrap()[0];
        assert_eq!(original, retained);
    }
}

#[test]
fn malformed_source_axis_ranges_return_an_error() {
    use write_fonts::FontBuilder;
    let font = FontRef::new(include_bytes!(
        "../test-data/fonts/cff2_fractional_blends.otf"
    ))
    .unwrap();
    let mut data = font
        .data_for_tag(Tag::new(b"fvar"))
        .unwrap()
        .as_bytes()
        .to_vec();
    let axis = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    // Make min=2 exceed max=1, without relying on a font writer's validation.
    data[axis + 4..axis + 8].copy_from_slice(&(2i32 << 16).to_be_bytes());
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        if record.tag() != Tag::new(b"fvar") {
            builder.add_raw(
                record.tag(),
                font.data_for_tag(record.tag()).unwrap().as_bytes(),
            );
        }
    }
    builder.add_raw(Tag::new(b"fvar"), data);
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    assert!(
        matches!(instance_font(&font, &parse_axis_limits("wght=0.5").unwrap()),
        Err(SubsetError::SubsetTableError(tag)) if tag == Tag::new(b"fvar"))
    );
}

#[test]
fn partial_cff2_stores_can_exceed_the_length_field() {
    use write_fonts::from_obj::ToOwnedTable;
    for (input, large_final) in [
        (
            include_bytes!("../test-data/fonts/cff2_large_store.otf").as_slice(),
            true,
        ),
        (
            include_bytes!("../test-data/fonts/cff2_unused_regions.otf").as_slice(),
            false,
        ),
    ] {
        let font = FontRef::new(input).unwrap();
        let bytes = instance_font(&font, &parse_axis_limits("wght=0:0.1:1").unwrap()).unwrap();
        let output = FontRef::new(&bytes).unwrap();
        let data = output.data_for_tag(Tag::new(b"CFF2")).unwrap();
        let cff = CffFontRef::new(data.as_bytes(), 0, None).unwrap();
        let store = cff.var_store().unwrap();
        let owned: write_fonts::tables::variations::ItemVariationStore = store.to_owned_table();
        let actual_length = write_fonts::dump_table(&owned).unwrap().len();
        assert_eq!(actual_length > u16::MAX as usize, large_final);
        let start =
            store.offset_data().as_bytes().as_ptr() as usize - data.as_bytes().as_ptr() as usize;
        let length = u16::from_be_bytes(data.as_bytes()[start - 2..start].try_into().unwrap());
        assert_eq!(length as usize, actual_length.min(u16::MAX as usize));
        for weight in [0., 0.5, 1.] {
            let mut coords = vec![
                read_fonts::types::F2Dot14::ZERO;
                output.fvar().unwrap().axes().unwrap().len()
            ];
            coords[0] = read_fonts::types::F2Dot14::from_f64(weight);
            let subfont = cff.subfont(0, &coords).unwrap();
            let mut path = Vec::<PathElement>::new();
            cff.draw(&subfont, GlyphId::new(1), &coords, None, &mut path)
                .unwrap();
            assert!(path.len() >= 4);
        }
    }
}
