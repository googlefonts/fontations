//! Exercise actual CFF outlines, independently of the legacy TTX goldens.
use skera::{subset_font, Plan, SubsetFlags, DEFAULT_LAYOUT_FEATURES};
use write_fonts::read::{
    collections::IntSet,
    model::glyph::outline::PathElement,
    ps::cff::CffFontRef,
    types::{GlyphId, NameId, Tag},
    FontRef, TableProvider,
};

fn plan(font: &FontRef, flags: SubsetFlags, unicodes: &str) -> Plan {
    Plan::new(
        &IntSet::empty(),
        &skera::parse_unicodes(unicodes).unwrap(),
        font,
        flags,
        &IntSet::empty(),
        &IntSet::all(),
        &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
        &IntSet::<NameId>::all(),
        &IntSet::all(),
    )
}

#[test]
fn cff_outlines_survive_all_subset_modes() {
    for filename in [
        "SourceSansPro-Regular.otf",
        "SourceHanSans-Regular_subset.otf",
        "NotoSerifMyanmar-Regular.otf",
        "AdobeVFPrototype.otf",
        "Cantarell-VF-ABC.otf",
        "NotoSansJP-VF.subset.otf",
        "cff1_seac.otf",
        "cff1_expert.otf",
        "cff1_flex.otf",
        "cff1_dotsect.otf",
    ] {
        let bytes = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let tag = if font.cff2().is_ok() {
            Tag::new(b"CFF2")
        } else {
            Tag::new(b"CFF ")
        };
        let original =
            CffFontRef::new(font.data_for_tag(tag).unwrap().as_bytes(), 0, None).unwrap();
        for flags in 0..8 {
            let f = SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
                | if flags & 1 != 0 {
                    SubsetFlags::SUBSET_FLAGS_NO_HINTING
                } else {
                    SubsetFlags::default()
                }
                | if flags & 2 != 0 {
                    SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE
                } else {
                    SubsetFlags::default()
                }
                | if flags & 4 != 0 {
                    SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
                } else {
                    SubsetFlags::default()
                };
            let p = plan(&font, f, "*");
            let out =
                subset_font(&font, &p).unwrap_or_else(|e| panic!("{filename} flags={flags}: {e}"));
            let subset = FontRef::new(&out).unwrap();
            let cff =
                CffFontRef::new(subset.data_for_tag(tag).unwrap().as_bytes(), 0, None).unwrap();
            for (old, new) in p.old_to_new_glyph_mapping() {
                let subfont = original
                    .subfont(original.subfont_index(old).unwrap(), &[])
                    .unwrap();
                let new_subfont = cff.subfont(cff.subfont_index(new).unwrap(), &[]).unwrap();
                let mut a = Vec::<PathElement>::new();
                let mut b = Vec::<PathElement>::new();
                original.draw(&subfont, old, &[], None, &mut a).unwrap();
                cff.draw(&new_subfont, new, &[], None, &mut b)
                    .unwrap_or_else(|e| panic!("{filename} flags={flags} glyph={old:?}: {e:?}"));
                assert_eq!(a, b, "{filename} flags={flags} glyph={old:?}");
                if tag == Tag::new(b"CFF ") {
                    assert_eq!(
                        original.evaluate_width(&subfont, old, &[]).unwrap(),
                        cff.evaluate_width(&new_subfont, new, &[]).unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn retained_gid_holes_and_notdef_are_empty() {
    for filename in ["SourceSansPro-Regular.otf", "AdobeVFPrototype.otf"] {
        let data = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&data).unwrap();
        let p = plan(&font, SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS, "41");
        let out = subset_font(&font, &p).unwrap();
        let subset = FontRef::new(&out).unwrap();
        let cff = CffFontRef::new(
            subset
                .data_for_tag(if font.cff2().is_ok() {
                    Tag::new(b"CFF2")
                } else {
                    Tag::new(b"CFF ")
                })
                .unwrap()
                .as_bytes(),
            0,
            None,
        )
        .unwrap();
        for gid in 0..cff.num_glyphs() {
            let gid = GlyphId::new(gid);
            if gid.to_u32() == 0 || !p.old_to_new_glyph_mapping().any(|(_, g)| g == gid) {
                let sf = cff.subfont(cff.subfont_index(gid).unwrap(), &[]).unwrap();
                let mut path = Vec::<PathElement>::new();
                cff.draw(&sf, gid, &[], None, &mut path).unwrap();
                assert!(path.is_empty());
            }
        }
    }
}

#[test]
fn cff2_subsetting_preserves_variation_locations() {
    use write_fonts::types::F2Dot14;
    for filename in [
        "AdobeVFPrototype.otf",
        "SourceSerif4Variable-Roman-HelloWorld.otf",
    ] {
        let data = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&data).unwrap();
        let original = CffFontRef::new(
            font.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
            0,
            None,
        )
        .unwrap();
        for flags in [
            SubsetFlags::default(),
            SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE,
            SubsetFlags::SUBSET_FLAGS_NO_HINTING,
            SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE | SubsetFlags::SUBSET_FLAGS_NO_HINTING,
        ] {
            let p = plan(&font, flags, "41,42,43,48,65,6C,6F,20,57,72,64");
            let out = subset_font(&font, &p).unwrap();
            let subset = FontRef::new(&out).unwrap();
            let cff = CffFontRef::new(
                subset.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
                0,
                None,
            )
            .unwrap();
            for coords in [[-16384, 0], [8192, 8192], [16384, 16384]] {
                let coords = coords.map(F2Dot14::from_bits);
                for (old, new) in p
                    .old_to_new_glyph_mapping()
                    .filter(|(_, g)| g.to_u32() != 0)
                {
                    let a = original
                        .subfont(original.subfont_index(old).unwrap(), &coords)
                        .unwrap();
                    let b = cff
                        .subfont(cff.subfont_index(new).unwrap(), &coords)
                        .unwrap();
                    let mut path_a = Vec::<PathElement>::new();
                    let mut path_b = Vec::<PathElement>::new();
                    original.draw(&a, old, &coords, None, &mut path_a).unwrap();
                    cff.draw(&b, new, &coords, None, &mut path_b).unwrap();
                    assert_eq!(path_a, path_b);
                }
            }
        }
    }
}

#[test]
fn seac_components_join_the_glyph_mapping() {
    let data = std::fs::read("test-data/fonts/cff1_seac.otf").unwrap();
    let font = FontRef::new(&data).unwrap();
    let p = plan(&font, SubsetFlags::default(), "C0");
    assert!(p.old_to_new_glyph_mapping().count() >= 4);
    let bytes = subset_font(&font, &p).unwrap();
    let out = FontRef::new(&bytes).unwrap();
    let cff = CffFontRef::new(
        out.data_for_tag(Tag::new(b"CFF ")).unwrap().as_bytes(),
        0,
        None,
    )
    .unwrap();
    for (_, gid) in p.old_to_new_glyph_mapping() {
        let sf = cff.subfont(cff.subfont_index(gid).unwrap(), &[]).unwrap();
        cff.draw(&sf, gid, &[], None, &mut Vec::<PathElement>::new())
            .unwrap();
    }
}

#[test]
fn full_cff2_instances_match_harfbuzz() {
    for (filename, request, full_average) in [
        ("AdobeVFPrototype.otf", "wght=650,CNTR=40", 529),
        ("Cantarell-VF-ABC.otf", "wght=650", 611),
        ("NotoSansJP-VF.subset.otf", "wght=500", 1000),
        (
            "SourceSerif4Variable-Roman-HelloWorld.otf",
            "wght=650,opsz=48",
            530,
        ),
    ] {
        let data = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&data).unwrap();
        let instanced =
            skera::instance_font(&font, &skera::parse_axis_limits(request).unwrap()).unwrap();
        let instance = FontRef::new(&instanced).unwrap();
        let p = plan(&instance, SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE, "*");
        let out = subset_font(&instance, &p).unwrap();
        let output = FontRef::new(&out).unwrap();
        let reference =
            std::fs::read(format!("test-data/expected/cff2-instances/hb-{filename}")).unwrap();
        let reference = FontRef::new(&reference).unwrap();
        let a = CffFontRef::new(
            output.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
            0,
            None,
        )
        .unwrap();
        let b = CffFontRef::new(
            reference
                .data_for_tag(Tag::new(b"CFF2"))
                .unwrap()
                .as_bytes(),
            0,
            None,
        )
        .unwrap();
        assert!(a.var_store().is_none());
        assert!(output.fvar().is_err());
        assert!(output.hvar().is_err());
        assert_eq!(
            instance.os2().unwrap().x_avg_char_width(),
            full_average,
            "{filename}"
        );
        assert_eq!(a.num_glyphs(), b.num_glyphs());
        for gid in 0..a.num_glyphs() {
            let gid = GlyphId::new(gid);
            let sa = a.subfont(a.subfont_index(gid).unwrap(), &[]).unwrap();
            let sb = b.subfont(b.subfont_index(gid).unwrap(), &[]).unwrap();
            let mut pa = Vec::<PathElement>::new();
            let mut pb = Vec::<PathElement>::new();
            a.draw(&sa, gid, &[], None, &mut pa).unwrap();
            b.draw(&sb, gid, &[], None, &mut pb).unwrap();
            assert_eq!(pa, pb, "{filename} {gid:?}");
            assert_eq!(
                output.hmtx().unwrap().advance(gid),
                reference.hmtx().unwrap().advance(gid)
            );
            assert_eq!(
                output.hmtx().unwrap().side_bearing(gid),
                reference.hmtx().unwrap().side_bearing(gid),
                "{filename} {gid:?}"
            );
            if let (Ok(a), Ok(b)) = (output.vmtx(), reference.vmtx()) {
                assert_eq!(a.advance(gid), b.advance(gid));
                assert_eq!(a.side_bearing(gid), b.side_bearing(gid));
            }
        }
    }
}

#[test]
fn partial_cff2_instances_match_harfbuzz() {
    use skrifa::MetadataProvider;
    use write_fonts::types::F2Dot14;
    for (i, (filename, request)) in [
        ("AdobeVFPrototype.otf", "wght=650"),
        ("AdobeVFPrototype.otf", "CNTR=40"),
        ("SourceSerif4Variable-Roman-HelloWorld.otf", "wght=650"),
        (
            "SourceSerif4Variable-Roman-HelloWorld.otf",
            "wght=300:550:700,opsz=12:30:48",
        ),
        ("Cantarell-VF-ABC.otf", "wght=200:500:700"),
        ("NotoSansJP-VF.subset.otf", "wght=200:500:700"),
        (
            "AdobeVFPrototype_vsindex.otf",
            "wght=300:500:700,CNTR=25:75",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let bytes =
            skera::instance_font(&font, &skera::parse_axis_limits(request).unwrap()).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let p = plan(&font, SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE, "*");
        let out = subset_font(&font, &p).unwrap();
        let output = FontRef::new(&out).unwrap();
        let bytes = std::fs::read(format!(
            "test-data/expected/cff2-instances/hb-partial-{i}.otf"
        ))
        .unwrap();
        let reference = FontRef::new(&bytes).unwrap();
        let a = CffFontRef::new(
            output.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
            0,
            None,
        )
        .unwrap();
        let b = CffFontRef::new(
            reference
                .data_for_tag(Tag::new(b"CFF2"))
                .unwrap()
                .as_bytes(),
            0,
            None,
        )
        .unwrap();
        assert_eq!(output.axes().len(), reference.axes().len());
        assert_eq!(a.num_glyphs(), b.num_glyphs());
        for fraction in [0.25, 0.75] {
            let settings: Vec<_> = output
                .axes()
                .iter()
                .map(|axis| {
                    (
                        axis.tag(),
                        axis.min_value() + (axis.max_value() - axis.min_value()) * fraction,
                    )
                })
                .collect();
            assert_eq!(
                output.axes().location(settings.iter().copied()).coords(),
                reference.axes().location(settings).coords(),
                "{filename} {request} user fraction={fraction}"
            );
        }
        for coord in [-1., -0.5, 0., 0.5, 1.] {
            let coords = vec![F2Dot14::from_f64(coord); output.axes().len()];
            for gid in 0..a.num_glyphs() {
                let gid = GlyphId::new(gid);
                let sa = a.subfont(a.subfont_index(gid).unwrap(), &coords).unwrap();
                let sb = b.subfont(b.subfont_index(gid).unwrap(), &coords).unwrap();
                let mut pa = Vec::<PathElement>::new();
                let mut pb = Vec::<PathElement>::new();
                a.draw(&sa, gid, &coords, None, &mut pa).unwrap();
                b.draw(&sb, gid, &coords, None, &mut pb).unwrap();
                assert_eq!(pa, pb, "{filename} {request} {coord} {gid:?}");
                assert_eq!(
                    output
                        .hvar()
                        .ok()
                        .and_then(|h| h.advance_delta(gid, &coords))
                        .map_or(0., |d| d.to_f64()),
                    reference
                        .hvar()
                        .ok()
                        .and_then(|h| h.advance_delta(gid, &coords))
                        .map_or(0., |d| d.to_f64()),
                    "{filename} {request} {coord} {gid:?}"
                );
            }
        }
        for (a, b) in output.axes().iter().zip(reference.axes().iter()) {
            assert_eq!(
                (a.tag(), a.min_value(), a.default_value(), a.max_value()),
                (b.tag(), b.min_value(), b.default_value(), b.max_value())
            );
        }
    }
}

#[test]
fn cff2_downgrade_preserves_outlines_and_cff1_widths() {
    for (filename, request) in [
        ("AdobeVFPrototype.otf", "wght=650,CNTR=40"),
        ("Cantarell-VF-ABC.otf", "wght=650"),
        ("NotoSansJP-VF.subset.otf", "wght=500"),
        (
            "SourceSerif4Variable-Roman-HelloWorld.otf",
            "wght=650,opsz=48",
        ),
    ] {
        let data = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&data).unwrap();
        assert!(skera::downgrade_cff2(&font).is_err());
        let bytes =
            skera::instance_font(&font, &skera::parse_axis_limits(request).unwrap()).unwrap();
        let instance = FontRef::new(&bytes).unwrap();
        let bytes = skera::downgrade_cff2(&instance).unwrap();
        let converted = FontRef::new(&bytes).unwrap();
        assert!(converted.cff2().is_err());
        let a = CffFontRef::new(
            instance.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
            0,
            None,
        )
        .unwrap();
        let b = CffFontRef::new(
            converted
                .data_for_tag(Tag::new(b"CFF "))
                .unwrap()
                .as_bytes(),
            0,
            None,
        )
        .unwrap();
        assert_eq!(a.num_glyphs(), b.num_glyphs());
        for gid in 0..a.num_glyphs() {
            let gid = GlyphId::new(gid);
            let sa = a.subfont(a.subfont_index(gid).unwrap(), &[]).unwrap();
            let sb = b.subfont(b.subfont_index(gid).unwrap(), &[]).unwrap();
            let mut pa = Vec::<PathElement>::new();
            let mut pb = Vec::<PathElement>::new();
            a.draw(&sa, gid, &[], None, &mut pa).unwrap();
            b.draw(&sb, gid, &[], None, &mut pb).unwrap();
            assert_eq!(pa, pb, "{filename} {gid:?}");
            assert_eq!(
                b.evaluate_width(&sb, gid, &[]).unwrap().unwrap().to_i32(),
                instance.hmtx().unwrap().advance(gid).unwrap() as i32
            );
        }
    }
}
