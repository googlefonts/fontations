//! Exercise component closure, remapping, auxiliary pruning, and actual paths.
use skera::{subset_font, Plan, SubsetFlags, DEFAULT_LAYOUT_FEATURES};
use skrifa::{
    instance::Size,
    outline::{DrawSettings, OutlinePen},
    MetadataProvider,
};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontRef, TableProvider},
    types::{GlyphId, Tag},
};

fn plan(font: &FontRef, gids: &str, flags: SubsetFlags) -> Plan {
    Plan::new(
        &skera::populate_gids(gids).unwrap(),
        &IntSet::empty(),
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
fn subset_paths_preserve_varc_conditions_axes_and_transforms() {
    for name in [
        "varc-6868",
        "varc-ac00-ac01",
        "varc-ac01-conditional",
        "varc-delta-precision",
        "varc-static-gvar",
    ] {
        let bytes = std::fs::read(format!("../font-test-data/test_data/ttf/{name}.ttf")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        for root in 0..font.maxp().unwrap().num_glyphs() {
            for retain in [false, true] {
                let flags = SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
                    | if retain {
                        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
                    } else {
                        SubsetFlags::default()
                    };
                let plan = plan(&font, &root.to_string(), flags);
                let bytes = subset_font(&font, &plan)
                    .unwrap_or_else(|e| panic!("{name} root={root} retain={retain}: {e}"));
                let subset = FontRef::new(&bytes).unwrap();
                for fraction in [0., 0.33, 0.66, 1.] {
                    let location = font.axes().location(font.axes().iter().map(|a| {
                        (
                            a.tag(),
                            a.min_value() + (a.max_value() - a.min_value()) * fraction,
                        )
                    }));
                    for (old, new) in plan.old_to_new_glyph_mapping() {
                        let mut a = Path::default();
                        let mut b = Path::default();
                        font.outline_glyphs()
                            .get(old)
                            .unwrap()
                            .draw(DrawSettings::unhinted(Size::unscaled(), &location), &mut a)
                            .unwrap();
                        subset
                            .outline_glyphs()
                            .get(new)
                            .unwrap()
                            .draw(DrawSettings::unhinted(Size::unscaled(), &location), &mut b)
                            .unwrap();
                        assert_eq!(
                            a, b,
                            "{name} root={root} retain={retain} glyph={old:?} fraction={fraction}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn auxiliary_tables_and_records_match_harfbuzz() {
    for (name, gids) in [
        ("varc-6868", "1"),
        ("varc-ac01-conditional", "1"),
        ("varc-ac00-ac01", "2"),
        ("varc-delta-precision", "2"),
        ("varc-delta-precision", "3"),
        ("varc-delta-precision", "4"),
        ("varc-static-gvar", "1"),
    ] {
        let bytes = std::fs::read(format!("../font-test-data/test_data/ttf/{name}.ttf")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        for retain in [false, true] {
            let suffix = if retain { "-retain" } else { "" };
            let reference =
                std::fs::read(format!("test-data/expected/varc/{name}-{gids}{suffix}.ttf"))
                    .unwrap();
            let expected = FontRef::new(&reference).unwrap();
            let flags = SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
                | if retain {
                    SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
                } else {
                    SubsetFlags::default()
                };
            let plan = plan(&font, gids, flags);
            let bytes = subset_font(&font, &plan).unwrap();
            let actual = FontRef::new(&bytes).unwrap();
            let a = actual.varc().unwrap();
            let e = expected.varc().unwrap();
            assert_eq!(
                a.coverage().unwrap().iter().collect::<Vec<_>>(),
                e.coverage().unwrap().iter().collect::<Vec<_>>(),
                "{name} gids={gids} retain={retain}"
            );
            let index = |a: write_fonts::read::tables::varc::Index2,
                         e: write_fonts::read::tables::varc::Index2| {
                assert_eq!(a.count(), e.count());
                for i in 0..a.count() as usize {
                    assert_eq!(
                        a.get(i).unwrap(),
                        e.get(i).unwrap(),
                        "{name} gids={gids} retain={retain} index={i}"
                    );
                }
            };
            index(
                a.var_composite_glyphs().unwrap(),
                e.var_composite_glyphs().unwrap(),
            );
            assert_eq!(
                a.axis_indices_list().is_some(),
                e.axis_indices_list().is_some()
            );
            if let Some(list) = a.axis_indices_list() {
                index(list.unwrap(), e.axis_indices_list().unwrap().unwrap());
            }
            assert_eq!(a.condition_list().is_some(), e.condition_list().is_some());
            if let Some(list) = a.condition_list() {
                let a: write_fonts::tables::varc::ConditionList = list.unwrap().to_owned_table();
                let e: write_fonts::tables::varc::ConditionList =
                    e.condition_list().unwrap().unwrap().to_owned_table();
                assert_eq!(a, e);
            }
            assert_eq!(a.multi_var_store().is_some(), e.multi_var_store().is_some());
            if let Some(store) = a.multi_var_store() {
                let a = store.unwrap();
                let e = e.multi_var_store().unwrap().unwrap();
                let ar: write_fonts::tables::varc::SparseVariationRegionList =
                    a.region_list().unwrap().to_owned_table();
                let er: write_fonts::tables::varc::SparseVariationRegionList =
                    e.region_list().unwrap().to_owned_table();
                assert_eq!(ar, er);
                assert_eq!(a.variation_data_count(), e.variation_data_count());
                for i in 0..a.variation_data_count() as usize {
                    let ad = a.variation_data().get(i).unwrap();
                    let ed = e.variation_data().get(i).unwrap();
                    assert_eq!(ad.region_indices(), ed.region_indices());
                    index(ad.delta_sets().unwrap(), ed.delta_sets().unwrap());
                }
            }
            assert_eq!(
                actual.maxp().unwrap().num_glyphs(),
                expected.maxp().unwrap().num_glyphs()
            );
            for gid in 0..actual.maxp().unwrap().num_glyphs() {
                let gid = GlyphId::new(gid as u32);
                assert_eq!(
                    actual.hmtx().unwrap().advance(gid),
                    expected.hmtx().unwrap().advance(gid)
                );
            }
        }
    }
}

#[test]
fn dropping_varc_skips_its_component_closure() {
    let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
    let plan = Plan::new(
        &skera::populate_gids("1").unwrap(),
        &IntSet::empty(),
        &font,
        SubsetFlags::default(),
        &[Tag::new(b"VARC")].into_iter().collect(),
        &IntSet::all(),
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
    );
    let bytes = subset_font(&font, &plan).unwrap();
    let subset = FontRef::new(&bytes).unwrap();
    assert!(subset.varc().is_err());
    assert_eq!(subset.maxp().unwrap().num_glyphs(), 2);
}

#[test]
fn varc_passthrough_requires_retained_glyph_ids() {
    let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
    let tag = Tag::new(b"VARC");
    let passthrough = [tag].into_iter().collect();
    for retain in [false, true] {
        let mut plan = plan(
            &font,
            "1",
            if retain {
                SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
            } else {
                SubsetFlags::default()
            },
        );
        plan.set_no_subset_tables(&passthrough);
        let bytes = subset_font(&font, &plan);
        if retain {
            let bytes = bytes.unwrap();
            let subset = FontRef::new(&bytes).unwrap();
            assert_eq!(
                font.data_for_tag(tag).unwrap().as_bytes(),
                subset.data_for_tag(tag).unwrap().as_bytes()
            );
        } else {
            assert!(matches!(bytes, Err(skera::SubsetError::SubsetTableError(t)) if t == tag));
        }
    }
}

#[test]
fn varc_components_include_truetype_composite_dependencies() {
    use write_fonts::{ps::cff::v2::Index, tables::varc::Varc, FontBuilder};
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let source = FontRef::new(&bytes).unwrap();
    let composite = GlyphId::new(3);
    let glyf = source.glyf().unwrap();
    let loca = source.loca(None).unwrap();
    let dependencies = match loca.get(composite, &glyf).unwrap().into_glyph().unwrap() {
        write_fonts::read::tables::glyf::Glyph::Composite(g) => g
            .components()
            .map(|c| GlyphId::from(c.glyph))
            .collect::<Vec<_>>(),
        _ => panic!("fixture glyph is not composite"),
    };
    let mut builder = FontBuilder::new();
    for r in source.table_directory().table_records() {
        builder.add_raw(r.tag(), source.data_for_tag(r.tag()).unwrap());
    }
    builder
        .add_table(&Varc::new(
            write_fonts::tables::layout::CoverageTable::from_iter([
                write_fonts::types::GlyphId16::new(1),
            ]),
            None,
            None,
            None,
            Index::from_items(vec![vec![0, 0, 3]]),
        ))
        .unwrap();
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    let plan = plan(&font, "1", SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE);
    let mapping = plan
        .old_to_new_glyph_mapping()
        .map(|(g, _)| g)
        .collect::<IntSet<_>>();
    assert!(mapping.contains(composite));
    for gid in dependencies {
        assert!(mapping.contains(gid));
    }
    let bytes = subset_font(&font, &plan).unwrap();
    let subset = FontRef::new(&bytes).unwrap();
    for (old, new) in plan.old_to_new_glyph_mapping() {
        let mut a = Path::default();
        let mut b = Path::default();
        font.outline_glyphs()
            .get(old)
            .unwrap()
            .draw(Size::unscaled(), &mut a)
            .unwrap();
        subset
            .outline_glyphs()
            .get(new)
            .unwrap()
            .draw(Size::unscaled(), &mut b)
            .unwrap();
        assert_eq!(a, b);
    }
}

#[test]
fn malformed_component_records_fail_without_losing_closure_errors() {
    use write_fonts::{ps::cff::v2::Index, tables::varc::Varc, FontBuilder};
    let source = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
    let mut builder = FontBuilder::new();
    for r in source.table_directory().table_records() {
        builder.add_raw(r.tag(), source.data_for_tag(r.tag()).unwrap());
    }
    builder
        .add_table(&Varc::new(
            write_fonts::tables::layout::CoverageTable::from_iter([
                write_fonts::types::GlyphId16::new(1),
            ]),
            None,
            None,
            None,
            Index::from_items(vec![vec![0, 0]]),
        ))
        .unwrap();
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    let plan = plan(&font, "1", SubsetFlags::default());
    assert!(
        matches!(subset_font(&font,&plan),Err(skera::SubsetError::SubsetTableError(tag)) if tag==Tag::new(b"VARC"))
    );
}

#[test]
fn null_conditions_preserve_component_paths_and_format_zero_is_invalid() {
    use write_fonts::{
        ps::cff::v2::Index,
        read::{FontData, FontRead},
        tables::{
            layout::Condition,
            varc::{ConditionList, Varc},
        },
        FontBuilder,
    };
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let source = FontRef::new(&bytes).unwrap();
    for kind in [0, 3, 4, 5, 255] {
        let table = Varc::new(
            write_fonts::tables::layout::CoverageTable::from_iter([
                write_fonts::types::GlyphId16::new(1),
            ]),
            None,
            Some(ConditionList::new(
                1,
                vec![Condition::format_2_variable_value(1, u32::MAX)],
            )),
            None,
            // HAVE_CONDITION, component glyph 3, condition index 0.
            Index::from_items(vec![vec![0x80, 0x80, 0, 3, 0]]),
        );
        let mut raw = write_fonts::dump_table(&table).unwrap();
        let varc = write_fonts::read::tables::varc::Varc::read(FontData::new(&raw)).unwrap();
        let list = varc.condition_list().unwrap().unwrap();
        let base = list.offset_data().as_bytes().as_ptr() as usize - raw.as_ptr() as usize;
        let offset = if kind == 0 {
            0
        } else {
            let offset = (raw.len() - base) as u32;
            match kind {
                3 | 4 => raw.extend([0, kind, 1, 0, 0, 0]),
                5 => raw.extend([0, 5, 0, 0, 0]),
                _ => raw.extend([0, 0]),
            }
            offset
        };
        raw[base + 4..base + 8].copy_from_slice(&offset.to_be_bytes());
        let mut builder = FontBuilder::new();
        for record in source.table_directory().table_records() {
            builder.add_raw(record.tag(), source.data_for_tag(record.tag()).unwrap());
        }
        builder.add_raw(Tag::new(b"VARC"), raw);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        for retain in [false, true] {
            let flags = SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
                | if retain {
                    SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
                } else {
                    SubsetFlags::default()
                };
            let plan = plan(&font, "1", flags);
            let subset_bytes = subset_font(&font, &plan);
            if kind == 255 {
                assert!(
                    matches!(subset_bytes, Err(skera::SubsetError::SubsetTableError(t)) if t == Tag::new(b"VARC"))
                );
                continue;
            }
            let subset_bytes = subset_bytes.unwrap();
            let subset = FontRef::new(&subset_bytes).unwrap();
            let new = plan
                .old_to_new_glyph_mapping()
                .find(|(g, _)| g.to_u32() == 1)
                .unwrap()
                .1;
            for weight in [100., 400., 900.] {
                let location = font.axes().location([(Tag::new(b"wght"), weight)]);
                let mut a = Path::default();
                let mut b = Path::default();
                font.outline_glyphs()
                    .get(GlyphId::new(1))
                    .unwrap()
                    .draw(DrawSettings::unhinted(Size::unscaled(), &location), &mut a)
                    .unwrap();
                subset
                    .outline_glyphs()
                    .get(new)
                    .unwrap()
                    .draw(DrawSettings::unhinted(Size::unscaled(), &location), &mut b)
                    .unwrap();
                assert_eq!(a, b, "kind={kind} retain={retain} weight={weight}");
                assert_eq!(a.0.is_empty(), kind == 5);
            }
        }
    }
}
