//! Large derived translations must survive full and composed instancing.
use skera::{instance_font, parse_axis_limits};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{FontRef, TableProvider},
    tables::colr::{Colr, Paint},
};
fn instance(bytes: &[u8], request: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
    .unwrap()
}
fn check(bytes: &[u8], expected: &[(f64, f64)]) {
    let colr: Colr = FontRef::new(bytes)
        .unwrap()
        .colr()
        .unwrap()
        .to_owned_table();
    assert!(colr.item_variation_store.is_none());
    let records = &colr
        .base_glyph_list
        .as_ref()
        .unwrap()
        .base_glyph_paint_records;
    assert_eq!(records.len(), expected.len());
    for (record, &(tx, ty)) in records.iter().zip(expected) {
        let (mut xx, mut yx, mut xy, mut yy, mut dx, mut dy) = (1., 0., 0., 1., 0., 0.);
        let mut paint = record.paint.as_ref();
        let mut depth = 0;
        while let Paint::Transform(v) = paint {
            let m = v.transform.as_ref();
            dx += xx * m.dx.to_f64() + xy * m.dy.to_f64();
            dy += yx * m.dx.to_f64() + yy * m.dy.to_f64();
            (xx, yx, xy, yy) = (
                xx * m.xx.to_f64() + xy * m.yx.to_f64(),
                yx * m.xx.to_f64() + yy * m.yx.to_f64(),
                xx * m.xy.to_f64() + xy * m.yy.to_f64(),
                yx * m.xy.to_f64() + yy * m.yy.to_f64(),
            );
            paint = v.paint.as_ref();
            depth += 1;
            assert!(depth <= 10);
        }
        assert!(matches!(paint, Paint::Glyph(_)));
        assert_eq!((xx, yx, xy, yy, dx, dy), (1., 0., 0., 1., tx, ty));
    }
}
#[test]
fn large_translations_preserve_offsets_and_composed_instances() {
    let expected = [
        (40000., -40000.),
        (-40000., 40000.),
        (4294967296., -4294967296.),
        (-4294967296., 4294967296.),
        (4294967296., 0.),
        (0., -4294967296.),
    ];
    let source = include_bytes!("../test-data/fonts/colr-large-translate.ttf");
    check(&instance(source, "wght=1,wdth=0"), &expected);
    check(&instance(&instance(source, "wght=1"), "wdth=0"), &expected);
    check(&instance(&instance(source, "wdth=0"), "wght=1"), &expected);
}
#[test]
fn maximum_long_word_row_preserves_large_translation() {
    let source = include_bytes!("../test-data/fonts/colr-maximum-translate.ttf");
    let expected = [(32766. * 1073741824., -32766. * 1073741824.)];
    check(&instance(source, "wght=1,wdth=0"), &expected);
    check(&instance(&instance(source, "wght=1"), "wdth=0"), &expected);
    check(&instance(&instance(source, "wdth=0"), "wght=1"), &expected);
}
