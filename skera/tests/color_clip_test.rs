//! Clip-box instancing must round fractional edges outwards.
use skera::{instance_font, parse_axis_limits};
use skrifa::{instance::Size, MetadataProvider};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{FontRef, TableProvider},
    tables::colr::{ClipBox, Colr},
    types::GlyphId,
};
const SOURCE: &[u8] = include_bytes!("../test-data/fonts/colr-clip-rounding.ttf");
const DELTAS: [[f32; 4]; 4] = [
    [-1., -1., 1., 1.],
    [1., 1., -1., -1.],
    [1., -1., 1., -1.],
    [-1., 1., -1., 1.],
];
fn expected(i: usize, weight: f32) -> [f32; 4] {
    let d = DELTAS[i];
    [
        (50. + d[0] * weight).floor(),
        (50. + d[1] * weight).floor(),
        (500. + d[2] * weight).ceil(),
        (500. + d[3] * weight).ceil(),
    ]
}
fn instance(bytes: &[u8], request: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
    .unwrap()
}
fn check(bytes: &[u8], weight: f32) {
    let font = FontRef::new(bytes).unwrap();
    let colr: Colr = font.colr().unwrap().to_owned_table();
    assert!(colr.item_variation_store.is_none());
    let clips = &colr.clip_list.as_ref().unwrap().clips;
    assert_eq!(clips.len(), 4);
    for (i, clip) in clips.iter().enumerate() {
        let ClipBox::Format1(v) = clip.clip_box.as_ref() else {
            panic!("static clip expected")
        };
        assert_eq!(
            [
                v.x_min.to_i16() as f32,
                v.y_min.to_i16() as f32,
                v.x_max.to_i16() as f32,
                v.y_max.to_i16() as f32
            ],
            expected(i, weight)
        );
    }
}
#[test]
fn clip_edges_expand_in_full_and_composed_instances() {
    for weight in [0., 0.25, 0.5, 0.75, 1.] {
        check(&instance(SOURCE, &format!("wght={weight},wdth=0")), weight);
        check(
            &instance(&instance(SOURCE, &format!("wght={weight}")), "wdth=0"),
            weight,
        );
        check(
            &instance(&instance(SOURCE, "wdth=0"), &format!("wght={weight}")),
            weight,
        );
    }
}
#[test]
fn runtime_clip_bounds_expand_before_size_scaling() {
    let font = FontRef::new(SOURCE).unwrap();
    for weight in [0., 0.25, 0.5, 0.75, 1.] {
        let location = font.axes().location([("wght", weight)]);
        for i in 0..4 {
            let glyph = font.color_glyphs().get(GlyphId::new(i as u32 + 1)).unwrap();
            for (size, factor) in [(Size::unscaled(), 1.), (Size::new(20.), 0.02)] {
                let bounds = glyph.bounding_box(&location, size).unwrap();
                let expected = expected(i, weight).map(|v| v * factor);
                assert_eq!(
                    [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max],
                    expected
                );
            }
        }
    }
}
