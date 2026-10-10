//! Custom mappings preserve glyph order while inserting empty output slots.
use skera::{subset_font, Plan, SubsetFlags, DEFAULT_LAYOUT_FEATURES};
use skrifa::{
    instance::Size,
    outline::{DrawSettings, OutlinePen},
    MetadataProvider,
};
use write_fonts::read::{collections::IntSet, types::GlyphId, FontRef, TableProvider};

fn plan(font: &FontRef, flags: SubsetFlags) -> Plan {
    Plan::new(
        &skera::populate_gids("1-8").unwrap(),
        &skera::parse_unicodes("20,41,42,43,61,62,63,301,627,628,915,916,2211,222b").unwrap(),
        font,
        flags,
        &IntSet::empty(),
        &IntSet::all(),
        &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
        &IntSet::all(),
        &IntSet::all(),
    )
}

#[derive(Default, Debug, PartialEq)]
struct Path(Vec<(u8, Vec<f32>)>);
impl OutlinePen for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push((0, vec![x, y]));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push((1, vec![x, y]));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.push((2, vec![cx, cy, x, y]));
    }
    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        self.0.push((3, vec![ax, ay, bx, by, x, y]));
    }
    fn close(&mut self) {
        self.0.push((4, vec![]));
    }
}

#[test]
fn gaps_preserve_outlines_cmap_and_variable_metrics() {
    for filename in [
        "RobotoFlex-Variable.ABC.ttf",
        "Cantarell-VF-ABC.otf",
        "SourceHanSans-Regular_subset.otf",
        "SourceSansPro-Regular.otf",
        "STIXTwoMath-Regular.ttf",
        "NotoSerifMyanmar-Regular.otf",
    ] {
        let bytes = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let mut p = plan(&font, SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE);
        let mut glyphs: Vec<_> = p.old_to_new_glyph_mapping().map(|(old, _)| old).collect();
        glyphs.sort_unstable();
        // Make the output longer than the source, including CFF/CFF2 sources.
        let offset = font.maxp().unwrap().num_glyphs() as u32 + 5;
        let mapping: Vec<_> = glyphs
            .iter()
            .enumerate()
            .map(|(index, &old)| {
                (
                    old,
                    GlyphId::new(if index == 0 {
                        0
                    } else {
                        offset + index as u32 * 3
                    }),
                )
            })
            .collect();
        let unicodes: Vec<_> = p.unicode_to_old_glyph_mapping().collect();
        p.set_glyph_mapping(&mapping).unwrap();
        let output = subset_font(&font, &p).unwrap_or_else(|e| panic!("{filename}: {e}"));
        let subset = FontRef::new(&output).unwrap();
        assert_eq!(
            subset.maxp().unwrap().num_glyphs() as u32,
            mapping.last().unwrap().1.to_u32() + 1
        );
        for (unicode, old) in unicodes {
            let new = mapping.iter().find(|&&(gid, _)| gid == old).unwrap().1;
            assert_eq!(subset.charmap().map(unicode), Some(new), "{filename}");
        }
        let used: IntSet<_> = mapping.iter().map(|&(_, new)| new).collect();
        if let Ok(glyf) = subset.glyf() {
            let loca = subset.loca(None).unwrap();
            for gid in 0..subset.maxp().unwrap().num_glyphs() {
                let gid = GlyphId::new(gid as u32);
                if !used.contains(gid) {
                    assert!(
                        loca.get(gid, &glyf).unwrap().into_glyph().is_none(),
                        "{filename}: {gid:?}"
                    );
                    assert_eq!(subset.hmtx().unwrap().advance(gid), Some(0));
                }
            }
        }
        for fraction in [0., 0.5, 1.] {
            let location = font.axes().location(font.axes().iter().map(|axis| {
                (
                    axis.tag(),
                    axis.min_value() + (axis.max_value() - axis.min_value()) * fraction,
                )
            }));
            let original_metrics = font.glyph_metrics(Size::unscaled(), &location);
            let subset_metrics = subset.glyph_metrics(Size::unscaled(), &location);
            for &(old, new) in &mapping {
                let mut original = Path::default();
                let mut actual = Path::default();
                font.outline_glyphs()
                    .get(old)
                    .unwrap()
                    .draw(
                        DrawSettings::unhinted(Size::unscaled(), &location),
                        &mut original,
                    )
                    .unwrap();
                subset
                    .outline_glyphs()
                    .get(new)
                    .unwrap()
                    .draw(
                        DrawSettings::unhinted(Size::unscaled(), &location),
                        &mut actual,
                    )
                    .unwrap();
                assert_eq!(actual, original, "{filename}: {old:?}, fraction={fraction}");
                assert_eq!(
                    subset_metrics.advance_width(new),
                    original_metrics.advance_width(old),
                    "{filename}: {old:?}"
                );
            }
        }
    }
}

#[test]
fn invalid_mappings_leave_the_plan_intact_and_empty_requests_reset_it() {
    let bytes = std::fs::read("test-data/fonts/RobotoFlex-Variable.ABC.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let mut p = plan(&font, SubsetFlags::default());
    let snapshot = |p: &Plan| {
        let mut map: Vec<_> = p.old_to_new_glyph_mapping().collect();
        map.sort_unstable();
        (map, p.unicode_to_old_glyph_mapping().collect::<Vec<_>>())
    };
    let initial = snapshot(&p);
    for request in [
        "0:1", "1:0", "1:3,2:3", "1:4,2:3", "2:4", "1:65535", "1:65534",
    ] {
        assert!(
            p.set_glyph_mapping(&skera::parse_glyph_mapping(request).unwrap())
                .is_err(),
            "{request}"
        );
        assert_eq!(snapshot(&p), initial, "{request}");
    }
    // Duplicate original IDs use the final request. A dropped glyph does not
    // increase the final glyph count or affect placement of closure glyphs.
    p.set_glyph_mapping(&skera::parse_glyph_mapping("1:2,1:3,99999:40000").unwrap())
        .unwrap();
    assert!(p
        .old_to_new_glyph_mapping()
        .any(|(old, new)| old == GlyphId::new(1) && new == GlyphId::new(3)));
    p.set_glyph_mapping(&[]).unwrap();
    assert_eq!(snapshot(&p), initial);
    let mut retained = plan(&font, SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS);
    assert!(retained
        .set_glyph_mapping(&skera::parse_glyph_mapping("1:3").unwrap())
        .is_err());
    retained.set_glyph_mapping(&[]).unwrap();
    assert!(retained
        .old_to_new_glyph_mapping()
        .all(|(old, new)| old == new));
    for input in ["1", "1:", ":2", "1:2:3", "-1:2", "1:-2", "1:2,"] {
        assert!(skera::parse_glyph_mapping(input).is_err(), "{input}");
    }
}

#[test]
fn layout_and_color_records_match_harfbuzz_with_gaps() {
    use write_fonts::from_obj::ToOwnedTable;
    for filename in [
        "gsub8_manually_created.otf",
        "gsub_alternate_substitution.otf",
        "gpos2_1_font7.otf",
        "gpos4_multiple_anchors_1.otf",
        "RobotoFlex-Variable.ABC.ttf",
        "BungeeColor-Regular.ttf",
    ] {
        let bytes = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let mut p = Plan::new(
            &IntSet::all(),
            &IntSet::empty(),
            &font,
            SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
        );
        let mapping: Vec<_> = (1..font.maxp().unwrap().num_glyphs().min(9) as u32)
            .map(|gid| (GlyphId::new(gid), GlyphId::new(gid * 3 + 1)))
            .collect();
        p.set_glyph_mapping(&mapping).unwrap();
        let actual = subset_font(&font, &p).unwrap();
        let actual = FontRef::new(&actual).unwrap();
        let reference =
            std::fs::read(format!("test-data/expected/glyph-mapping/{filename}")).unwrap();
        let reference = FontRef::new(&reference).unwrap();
        assert_eq!(
            actual.maxp().unwrap().num_glyphs(),
            reference.maxp().unwrap().num_glyphs()
        );
        macro_rules! compare {
            ($accessor:ident, $table:path) => {
                let a: Option<$table> = actual.$accessor().ok().map(|table| table.to_owned_table());
                let b: Option<$table> = reference
                    .$accessor()
                    .ok()
                    .map(|table| table.to_owned_table());
                assert_eq!(a, b, "{filename}: {}", stringify!($accessor));
            };
        }
        compare!(gsub, write_fonts::tables::gsub::Gsub);
        compare!(gpos, write_fonts::tables::gpos::Gpos);
        compare!(gdef, write_fonts::tables::gdef::Gdef);
        compare!(colr, write_fonts::tables::colr::Colr);
    }
}

#[test]
fn retaining_the_source_glyph_count_adds_empty_trailing_glyphs() {
    for filename in [
        "RobotoFlex-Variable.ABC.ttf",
        "Muli-ABC.ttf",
        "Cantarell-VF-ABC.otf",
        "SourceHanSans-Regular_subset.otf",
        "SourceSansPro-Regular.otf",
    ] {
        let bytes = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let make_plan = |flags| {
            Plan::new(
                &skera::populate_gids("0,1").unwrap(),
                &IntSet::empty(),
                &font,
                flags,
                &IntSet::empty(),
                &IntSet::all(),
                &IntSet::all(),
                &IntSet::all(),
                &IntSet::all(),
            )
        };
        let flags =
            SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE;
        let reduced = make_plan(flags);
        let mut retained = make_plan(flags | SubsetFlags::SUBSET_FLAGS_RETAIN_NUM_GLYPHS);
        // Resetting an empty mapping must also retain the source count.
        retained.set_glyph_mapping(&[]).unwrap();
        let expected = subset_font(&font, &reduced).unwrap();
        let expected = FontRef::new(&expected).unwrap();
        let actual = subset_font(&font, &retained).unwrap();
        let actual = FontRef::new(&actual).unwrap();
        let count = font.maxp().unwrap().num_glyphs();
        assert_eq!(actual.maxp().unwrap().num_glyphs(), count, "{filename}");
        assert!(expected.maxp().unwrap().num_glyphs() < count);
        let used: IntSet<_> = retained
            .old_to_new_glyph_mapping()
            .map(|(_, gid)| gid)
            .collect();
        let mut empty = Path::default();
        for gid in 0..count {
            let gid = GlyphId::new(gid as u32);
            let mut path = Path::default();
            actual
                .outline_glyphs()
                .get(gid)
                .unwrap()
                .draw(DrawSettings::unhinted(Size::unscaled(), &[][..]), &mut path)
                .unwrap();
            if used.contains(gid) {
                empty.0.clear();
                expected
                    .outline_glyphs()
                    .get(gid)
                    .unwrap()
                    .draw(
                        DrawSettings::unhinted(Size::unscaled(), &[][..]),
                        &mut empty,
                    )
                    .unwrap();
                assert_eq!(path, empty, "{filename}: {gid:?}");
                assert_eq!(
                    actual.hmtx().unwrap().advance(gid),
                    expected.hmtx().unwrap().advance(gid)
                );
            } else {
                assert!(path.0.is_empty(), "{filename}: {gid:?}");
                assert_eq!(actual.hmtx().unwrap().advance(gid), Some(0));
                if let Ok(gvar) = actual.gvar() {
                    assert!(gvar.data_for_gid(gid).unwrap().is_none());
                }
            }
        }
        let dense = make_plan(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE);
        let ignored = make_plan(
            SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE | SubsetFlags::SUBSET_FLAGS_RETAIN_NUM_GLYPHS,
        );
        assert_eq!(
            subset_font(&font, &dense).unwrap(),
            subset_font(&font, &ignored).unwrap()
        );
    }
}

#[test]
fn iftb_uses_long_outline_offsets_without_changing_geometry() {
    use write_fonts::read::ps::cff::CffFontRef;
    for filename in [
        "RobotoFlex-Variable.ABC.ttf",
        "Muli-ABC.ttf",
        "Cantarell-VF-ABC.otf",
        "SourceHanSans-Regular_subset.otf",
        "SourceSansPro-Regular.otf",
    ] {
        let bytes = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        for retain in [false, true] {
            let flags = SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
                | if retain {
                    SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
                } else {
                    SubsetFlags::default()
                };
            let compact = subset_font(&font, &plan(&font, flags)).unwrap();
            let long = subset_font(
                &font,
                &plan(&font, flags | SubsetFlags::SUBSET_FLAGS_IFTB_REQUIREMENTS),
            )
            .unwrap();
            let compact = FontRef::new(&compact).unwrap();
            let long = FontRef::new(&long).unwrap();
            if long.glyf().is_ok() {
                assert_eq!(long.head().unwrap().index_to_loc_format(), 1);
                assert_eq!(compact.head().unwrap().index_to_loc_format(), 0);
                let count = long.maxp().unwrap().num_glyphs() as usize;
                assert_eq!(
                    long.data_for_tag(write_fonts::types::Tag::new(b"loca"))
                        .unwrap()
                        .len(),
                    (count + 1) * 4
                );
                if let Ok(gvar) = long.gvar() {
                    assert_eq!(gvar.flags().bits() & 1, 1);
                    assert_eq!(
                        gvar.glyph_variation_data_offsets_byte_range().len(),
                        (count + 1) * 4
                    );
                    assert!(
                        gvar.shared_tuples_offset().to_u32()
                            <= gvar.glyph_variation_data_array_offset()
                    );
                }
            } else {
                let tag = if long.cff2().is_ok() {
                    write_fonts::types::Tag::new(b"CFF2")
                } else {
                    write_fonts::types::Tag::new(b"CFF ")
                };
                let a = CffFontRef::new(compact.data_for_tag(tag).unwrap().as_bytes(), 0, None)
                    .unwrap();
                let b =
                    CffFontRef::new(long.data_for_tag(tag).unwrap().as_bytes(), 0, None).unwrap();
                assert_eq!(b.charstrings().off_size(), 4);
                assert!(a.charstrings().off_size() < 4);
                assert_eq!(a.charstrings().count(), b.charstrings().count());
                for gid in 0..a.charstrings().count() as usize {
                    assert_eq!(
                        a.charstrings().get(gid),
                        b.charstrings().get(gid),
                        "{filename}: {gid}"
                    );
                }
            }
            for fraction in [0., 0.5, 1.] {
                let location = font.axes().location(font.axes().iter().map(|axis| {
                    (
                        axis.tag(),
                        axis.min_value() + (axis.max_value() - axis.min_value()) * fraction,
                    )
                }));
                for (_, gid) in plan(&font, flags).old_to_new_glyph_mapping() {
                    let mut a = Path::default();
                    let mut b = Path::default();
                    compact
                        .outline_glyphs()
                        .get(gid)
                        .unwrap()
                        .draw(DrawSettings::unhinted(Size::unscaled(), &location), &mut a)
                        .unwrap();
                    long.outline_glyphs()
                        .get(gid)
                        .unwrap()
                        .draw(DrawSettings::unhinted(Size::unscaled(), &location), &mut b)
                        .unwrap();
                    assert_eq!(a, b, "{filename}: {gid:?}, fraction={fraction}");
                }
            }
        }
    }
}
