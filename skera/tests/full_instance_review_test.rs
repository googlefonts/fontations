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
fn variable_color_requires_its_instancing_handler() {
    let bytes = include_bytes!("../test-data/fonts/cff2_variable_color.otf");
    let font = FontRef::new(bytes).unwrap();
    assert!(matches!(
        instance_font(&font, &parse_axis_limits("wght=0.5").unwrap()),
        Err(SubsetError::SubsetTableError(tag)) if tag == Tag::new(b"COLR")
    ));
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
