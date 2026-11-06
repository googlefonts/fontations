//! Negative derived radii need circles and color stops reparameterized together.
use skera::{instance_font, parse_axis_limits};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{FontRef, TableProvider},
    tables::colr::{Colr, Extend, Paint, PaintRadialGradient},
};

const SOURCE: &[u8] = include_bytes!("../test-data/fonts/colr-negative-radii.ttf");
fn instance(bytes: &[u8], request: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
    .unwrap()
}
fn sample(g: [f64; 6], stops: &[(f64, f64)], extend: Extend, x: f64, y: f64) -> Option<f64> {
    let [x0, y0, r0, x1, y1, r1] = g;
    let (dx, dy, dr, px, py) = (x1 - x0, y1 - y0, r1 - r0, x - x0, y - y0);
    let a = dx * dx + dy * dy - dr * dr;
    let b = -2. * (px * dx + py * dy + r0 * dr);
    let c = px * px + py * py - r0 * r0;
    let disc = b * b - 4. * a * c;
    if disc < 0. {
        return None;
    }
    let mut roots = [(-b - disc.sqrt()) / (2. * a), (-b + disc.sqrt()) / (2. * a)]
        .into_iter()
        .filter(|t| r0 + t * dr >= -1e-8);
    let first_root = roots.next()?;
    let mut t = roots.fold(first_root, f64::max);
    let first = stops[0].0;
    let last = stops.last()?.0;
    let width = last - first;
    if extend == Extend::Repeat {
        t = first + (t - first).rem_euclid(width);
    }
    if extend == Extend::Reflect {
        let q = (t - first).rem_euclid(2. * width);
        t = first + if q <= width { q } else { 2. * width - q };
    }
    if t < first {
        return Some(stops[0].1);
    }
    for pair in stops.windows(2) {
        let [(offset0, alpha0), (offset1, alpha1)] = pair else {
            unreachable!()
        };
        if t < *offset1 {
            return Some(alpha0 + (t - offset0) / (offset1 - offset0) * (alpha1 - alpha0));
        }
    }
    Some(stops.last()?.1)
}
fn actual_sample(paint: &Paint, x: f64, y: f64) -> Option<f64> {
    match paint {
        Paint::Solid(paint) if paint.alpha.to_f32() == 0. => None,
        Paint::RadialGradient(paint) => {
            let PaintRadialGradient {
                x0,
                y0,
                radius0,
                x1,
                y1,
                radius1,
                color_line,
            } = paint;
            let g = [
                f64::from(x0.to_i16()),
                f64::from(y0.to_i16()),
                f64::from(radius0.to_u16()),
                f64::from(x1.to_i16()),
                f64::from(y1.to_i16()),
                f64::from(radius1.to_u16()),
            ];
            let stops: Vec<_> = color_line
                .color_stops
                .iter()
                .map(|stop| (stop.stop_offset.to_f64(), stop.alpha.to_f64()))
                .collect();
            sample(g, &stops, color_line.extend, x, y)
        }
        _ => panic!("expected static radial or transparent paint"),
    }
}
#[test]
fn full_and_composed_negative_radial_instances_preserve_samples() {
    let full = instance(SOURCE, "wght=1,wdth=0");
    let colr: Colr = FontRef::new(&full)
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
    assert_eq!(records.len(), 30);
    for (i, record) in records.iter().enumerate() {
        let (r0, r1) = [
            (-100., 500.),
            (500., -100.),
            (-100., -200.),
            (-200., -100.),
            (-100., -100.),
        ][i % 5];
        let extend = [Extend::Pad, Extend::Repeat, Extend::Reflect][i / 5 % 3];
        for x in [-2000., -1000., -200., 0., 100., 400., 800., 1500.] {
            for y in [-1200., -100., 0., 100., 300., 1500.] {
                let expected = sample(
                    [10., 15., r0, 410., 215., r1],
                    &[(0., 0.25), (0.5, 0.5), (1., 1.)],
                    extend,
                    x,
                    y,
                );
                let actual = actual_sample(record.paint.as_ref(), x, y);
                match (actual, expected) {
                    (Some(a), Some(b)) => {
                        assert!((a - b).abs() < 1e-9, "paint {i} at ({x}, {y}): {a} vs {b}")
                    }
                    (None, None) => {}
                    mismatch => panic!("paint {i} at ({x}, {y}): {mismatch:?}"),
                }
            }
        }
    }
    for (first, last) in [("wght=1", "wdth=0"), ("wdth=0", "wght=1")] {
        let bytes = instance(&instance(SOURCE, first), last);
        let composed: Colr = FontRef::new(&bytes)
            .unwrap()
            .colr()
            .unwrap()
            .to_owned_table();
        assert_eq!(composed, colr);
    }
}
#[test]
fn fractional_parameter_shifts_preserve_stop_precision() {
    let source = include_bytes!("../test-data/fonts/colr-negative-radii-precision.ttf");
    let bytes = instance(source, "wght=1,wdth=0");
    let colr: Colr = FontRef::new(&bytes)
        .unwrap()
        .colr()
        .unwrap()
        .to_owned_table();
    let Paint::RadialGradient(paint) = colr
        .base_glyph_list
        .as_ref()
        .unwrap()
        .base_glyph_paint_records[0]
        .paint
        .as_ref()
    else {
        panic!()
    };
    assert_eq!((paint.radius0.to_u16(), paint.radius1.to_u16()), (650, 50));
    assert_eq!(
        paint
            .color_line
            .color_stops
            .iter()
            .map(|stop| stop.stop_offset.to_bits())
            .collect::<Vec<_>>(),
        vec![4096, 4097, 20480]
    );
    for (first, last) in [("wght=1", "wdth=0"), ("wdth=0", "wght=1")] {
        let bytes = instance(&instance(source, first), last);
        let composed: Colr = FontRef::new(&bytes)
            .unwrap()
            .colr()
            .unwrap()
            .to_owned_table();
        assert_eq!(composed, colr);
    }
}
