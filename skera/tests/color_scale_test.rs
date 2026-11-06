//! Derived COLRv1 scales need wider storage when all axes are pinned.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontRef, TableProvider},
    tables::colr::{Colr, Paint},
    types::Fixed,
};

const SOURCE: &[u8] = include_bytes!("../test-data/fonts/colr-wide-scale.ttf");

fn instance(bytes: &[u8], request: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
    .unwrap()
}

fn check(bytes: &[u8]) {
    let font = FontRef::new(bytes).unwrap();
    let colr: Colr = font.colr().unwrap().to_owned_table();
    assert!(colr.item_variation_store.is_none());
    let list = colr.base_glyph_list.as_ref().unwrap();
    assert_eq!(list.base_glyph_paint_records.len(), 8);
    for (i, record) in list.base_glyph_paint_records.iter().enumerate() {
        let Paint::Transform(paint) = record.paint.as_ref() else {
            panic!("scale {i} must use the wider affine transform");
        };
        let scale = if i % 8 < 4 { 2.5 } else { -2.5 };
        let sy = if i % 4 < 2 { -scale } else { scale };
        let (cx, cy) = if i % 2 == 1 { (40., -34.) } else { (0., 0.) };
        let matrix = paint.transform.as_ref();
        assert_eq!(matrix.xy, Fixed::ZERO);
        assert_eq!(matrix.yx, Fixed::ZERO);
        assert_eq!((matrix.xx.to_f64(), matrix.yy.to_f64()), (scale, sy));
        assert_eq!(
            (matrix.dx.to_f64(), matrix.dy.to_f64()),
            (cx * (1. - scale), cy * (1. - sy))
        );
        assert!(matches!(paint.paint.as_ref(), Paint::Solid(_)));
    }
}

#[test]
fn full_and_composed_instances_preserve_wide_and_centered_scales() {
    let full = instance(SOURCE, "wght=1,wdth=0");
    check(&full);
    let partial = instance(SOURCE, "wdth=0");
    let composed = instance(&partial, "wght=1");
    check(&composed);
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
    check(&subset_font(&font, &plan).unwrap());
}
