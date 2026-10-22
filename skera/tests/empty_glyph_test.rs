//! Zero-contour glyph instructions can adjust phantom-point metrics.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::read::{collections::IntSet, tables::glyf::Glyph, FontRef, TableProvider};
use write_fonts::types::GlyphId;

const SOURCE: &[u8] = include_bytes!("../test-data/fonts/empty-hinted.ttf");
const INSTRUCTIONS: &[u8] = &[0xb0, 1, 0x21];

fn check(bytes: &[u8], hints: bool, advance: u16) {
    let font = FontRef::new(bytes).unwrap();
    let gid = GlyphId::new(1);
    let glyf = font.glyf().unwrap();
    match font
        .loca(None)
        .unwrap()
        .get(gid, &glyf)
        .unwrap()
        .into_glyph()
    {
        Some(Glyph::Simple(glyph)) => {
            assert_eq!(glyph.number_of_contours(), 0);
            assert_eq!(glyph.instructions(), if hints { INSTRUCTIONS } else { &[] });
        }
        None => assert!(!hints),
        _ => panic!("expected zero contours"),
    }
    assert_eq!(font.hmtx().unwrap().advance(gid), Some(advance));
}

fn subset(bytes: &[u8], flags: SubsetFlags) -> Vec<u8> {
    let font = FontRef::new(bytes).unwrap();
    let plan = Plan::new(
        &IntSet::all(),
        &IntSet::all(),
        &font,
        flags,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
    );
    subset_font(&font, &plan).unwrap()
}

#[test]
fn zero_contour_instructions_survive_subsetting_and_overlap_flags() {
    check(SOURCE, true, 500);
    for flags in [
        SubsetFlags::default(),
        SubsetFlags::SUBSET_FLAGS_SET_OVERLAPS_FLAG,
        SubsetFlags::SUBSET_FLAGS_NO_HINTING,
        SubsetFlags::SUBSET_FLAGS_NO_HINTING | SubsetFlags::SUBSET_FLAGS_SET_OVERLAPS_FLAG,
    ] {
        check(
            &subset(SOURCE, flags),
            !flags.contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING),
            500,
        );
    }
}

#[test]
fn zero_contour_instructions_survive_full_partial_and_composed_instances() {
    for (request, advance) in [
        ("wght=1", 550),
        ("wght=1,wdth=0", 550),
        ("wght=0,wdth=0", 500),
    ] {
        let bytes = instance_font(
            &FontRef::new(SOURCE).unwrap(),
            &parse_axis_limits(request).unwrap(),
        )
        .unwrap();
        check(&bytes, true, advance);
        check(&subset(&bytes, SubsetFlags::default()), true, advance);
        check(
            &subset(&bytes, SubsetFlags::SUBSET_FLAGS_NO_HINTING),
            false,
            advance,
        );
    }
    let partial = instance_font(
        &FontRef::new(SOURCE).unwrap(),
        &parse_axis_limits("wght=1").unwrap(),
    )
    .unwrap();
    let full = instance_font(
        &FontRef::new(&partial).unwrap(),
        &parse_axis_limits("wdth=0").unwrap(),
    )
    .unwrap();
    check(&full, true, 550);
}
