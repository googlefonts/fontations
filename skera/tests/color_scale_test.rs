//! Derived COLRv1 scales need wider storage when all axes are pinned.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontRef, TableProvider},
    tables::colr::{Colr, Paint},
};

const SOURCE: &[u8] = include_bytes!("../test-data/fonts/colr-wide-scale.ttf");

fn instance(bytes: &[u8], request: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
    .unwrap()
}

fn check(bytes: &[u8], magnitude: f64, count: usize) {
    let font = FontRef::new(bytes).unwrap();
    let colr: Colr = font.colr().unwrap().to_owned_table();
    assert!(colr.item_variation_store.is_none());
    let list = colr.base_glyph_list.as_ref().unwrap();
    assert_eq!(list.base_glyph_paint_records.len(), count);
    for (i, record) in list.base_glyph_paint_records.iter().enumerate() {
        let Paint::Transform(_) = record.paint.as_ref() else {
            panic!("scale {i} must use the wider affine transform");
        };
        let scale = if i % 8 < 4 { magnitude } else { -magnitude };
        let sy = if i % 4 < 2 { -scale } else { scale };
        let (cx, cy) = if i % 2 == 1 {
            if i < 8 {
                (40., -34.)
            } else {
                (30003., -30005.)
            }
        } else {
            (0., 0.)
        };
        let (mut xx, mut yx, mut xy, mut yy, mut dx, mut dy) = (1., 0., 0., 1., 0., 0.);
        let mut child = record.paint.as_ref();
        let mut depth = 0;
        while let Paint::Transform(paint) = child {
            let matrix = paint.transform.as_ref();
            dx += xx * matrix.dx.to_f64() + xy * matrix.dy.to_f64();
            dy += yx * matrix.dx.to_f64() + yy * matrix.dy.to_f64();
            let next = (
                xx * matrix.xx.to_f64() + xy * matrix.yx.to_f64(),
                yx * matrix.xx.to_f64() + yy * matrix.yx.to_f64(),
                xx * matrix.xy.to_f64() + xy * matrix.yy.to_f64(),
                yx * matrix.xy.to_f64() + yy * matrix.yy.to_f64(),
            );
            (xx, yx, xy, yy) = next;
            child = paint.paint.as_ref();
            depth += 1;
            assert!(depth <= 20);
        }
        assert_eq!((xx, yy), (scale, sy));
        assert_eq!((xy, yx), (0., 0.));
        assert_eq!((dx, dy), (cx * (1. - scale), cy * (1. - sy)));
        assert!(matches!(child, Paint::Solid(_)));
    }
}

#[test]
fn full_and_composed_instances_preserve_wide_and_centered_scales() {
    let full = instance(SOURCE, "wght=1,wdth=0");
    check(&full, 2.5, 16);
    let partial = instance(SOURCE, "wdth=0");
    let composed = instance(&partial, "wght=1");
    check(&composed, 2.5, 16);
    let direct: Colr = FontRef::new(&full)
        .unwrap()
        .colr()
        .unwrap()
        .to_owned_table();
    let composed: Colr = FontRef::new(&composed)
        .unwrap()
        .colr()
        .unwrap()
        .to_owned_table();
    assert_eq!(direct, composed);

    let font = FontRef::new(&full).unwrap();
    let plan = Plan::new(
        &IntSet::all(),
        &IntSet::all(),
        &font,
        SubsetFlags::default(),
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
    );
    check(&subset_font(&font, &plan).unwrap(), 2.5, 16);
}

#[test]
fn scales_exceeding_fixed_storage_survive_full_and_composed_instances() {
    let source = include_bytes!("../test-data/fonts/colr-large-scale.ttf");
    check(&instance(source, "wght=1,wdth=0"), 131072., 16);
    check(
        &instance(&instance(source, "wght=1"), "wdth=0"),
        131072.,
        16,
    );
    check(
        &instance(&instance(source, "wdth=0"), "wght=1"),
        131072.,
        16,
    );
}

#[test]
fn maximum_long_word_row_preserves_large_scale() {
    let source = include_bytes!("../test-data/fonts/colr-maximum-scale.ttf");
    check(&instance(source, "wght=1,wdth=0"), 4294705152., 1);
}
