//! Affine instancing preserves low Fixed bits at ordinary magnitudes.
use skera::{instance_font, parse_axis_limits};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{FontRef, TableProvider},
    tables::colr::{Colr, Paint},
};
fn instance(bytes: &[u8], request: &str) -> Result<Vec<u8>, skera::SubsetError> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
}
fn check(bytes: &[u8]) {
    let colr: Colr = FontRef::new(bytes)
        .unwrap()
        .colr()
        .unwrap()
        .to_owned_table();
    assert!(colr.item_variation_store.is_none());
    let paints = &colr
        .base_glyph_list
        .as_ref()
        .unwrap()
        .base_glyph_paint_records;
    assert_eq!(paints.len(), 28);
    for (i, record) in paints.iter().enumerate() {
        let Paint::Transform(paint) = record.paint.as_ref() else {
            panic!()
        };
        let t = paint.transform.as_ref();
        let actual = [
            t.xx.to_bits(),
            t.yx.to_bits(),
            t.xy.to_bits(),
            t.yy.to_bits(),
            t.dx.to_bits(),
            t.dy.to_bits(),
        ];
        let mut expected = [65536, 0, 0, 65536, 0, 0];
        let sign = if i % 4 < 2 { 1 } else { -1 };
        let delta = if i % 2 == 0 { 1 } else { -1 };
        if i < 24 {
            expected[i / 4] = sign * ((if i / 4 < 4 { 1 } else { 1000 }) * 65536 + 1) + delta;
        } else {
            expected[4] = sign * (1000 * 65536 + if i % 2 == 0 { 1 } else { 2 });
        }
        assert_eq!(actual, expected, "affine {i}");
    }
}
#[test]
fn affine_low_bits_survive_full_and_composed_instances() {
    let source = include_bytes!("../test-data/fonts/colr-affine-values.ttf");
    check(&instance(source, "wght=1,wdth=0").unwrap());
    check(&instance(&instance(source, "wght=1").unwrap(), "wdth=0").unwrap());
    check(&instance(&instance(source, "wdth=0").unwrap(), "wght=1").unwrap());
}
