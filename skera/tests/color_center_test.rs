//! Derived transform centers must preserve the operation around that point.
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
fn check(bytes: &[u8]) {
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
    assert_eq!(records.len(), 16);
    let centers = [
        (40000., -40000.),
        (-40000., 40000.),
        (4294967297., -4294967299.),
        (-4294967297., 4294967299.),
    ];
    for (i, record) in records.iter().enumerate() {
        let (cx, cy) = centers[i % 4];
        let linear = match i / 4 {
            0 => [1.5, 0., 0., 0.5],
            1 => [1.5, 0., 0., 1.5],
            2 => [0., 1., -1., 0.],
            _ => [1., -1., -1., 1.],
        };
        let [a, b, c, d] = linear;
        let expected = [a, b, c, d, cx - a * cx - c * cy, cy - b * cx - d * cy];
        let mut matrix = [1., 0., 0., 1., 0., 0.];
        let mut paint = record.paint.as_ref();
        let mut depth = 0;
        loop {
            let (values, child) = match paint {
                Paint::Transform(v) => {
                    let m = v.transform.as_ref();
                    (
                        [
                            m.xx.to_f64(),
                            m.yx.to_f64(),
                            m.xy.to_f64(),
                            m.yy.to_f64(),
                            m.dx.to_f64(),
                            m.dy.to_f64(),
                        ],
                        v.paint.as_ref(),
                    )
                }
                Paint::Rotate(v) => {
                    assert_eq!(v.angle.to_f64(), 0.5);
                    ([0., 1., -1., 0., 0., 0.], v.paint.as_ref())
                }
                Paint::Skew(v) => {
                    assert_eq!(
                        (v.x_skew_angle.to_f64(), v.y_skew_angle.to_f64()),
                        (0.25, -0.25)
                    );
                    ([1., -1., -1., 1., 0., 0.], v.paint.as_ref())
                }
                Paint::Glyph(_) => break,
                _ => panic!("unexpected paint at center {i}"),
            };
            let [xx, yx, xy, yy, dx, dy] = matrix;
            let [a, b, c, d, tx, ty] = values;
            matrix = [
                xx * a + xy * b,
                yx * a + yy * b,
                xx * c + xy * d,
                yx * c + yy * d,
                dx + xx * tx + xy * ty,
                dy + yx * tx + yy * ty,
            ];
            paint = child;
            depth += 1;
            assert!(depth <= 15);
        }
        assert_eq!(matrix, expected, "center {i}");
    }
}
#[test]
fn wide_centers_preserve_scale_rotation_skew_and_composed_instances() {
    let source = include_bytes!("../test-data/fonts/colr-wide-centers.ttf");
    check(&instance(source, "wght=1,wdth=0"));
    check(&instance(&instance(source, "wght=1"), "wdth=0"));
    check(&instance(&instance(source, "wdth=0"), "wght=1"));
}
