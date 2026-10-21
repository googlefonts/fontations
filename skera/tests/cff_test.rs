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
fn shared_hint_operands_preserve_both_glyphs() {
    let bytes = include_bytes!("../test-data/fonts/cff1_shared_hint_operands.otf");
    let font = FontRef::new(bytes).unwrap();
    let p = plan(&font, SubsetFlags::SUBSET_FLAGS_NO_HINTING, "41,42");
    let bytes = subset_font(&font, &p).unwrap();
    let out = FontRef::new(&bytes).unwrap();
    let original = CffFontRef::new(
        font.data_for_tag(Tag::new(b"CFF ")).unwrap().as_bytes(),
        0,
        None,
    )
    .unwrap();
    let subset = CffFontRef::new(
        out.data_for_tag(Tag::new(b"CFF ")).unwrap().as_bytes(),
        0,
        None,
    )
    .unwrap();
    for (old, new) in p
        .old_to_new_glyph_mapping()
        .filter(|(_, g)| g.to_u32() != 0)
    {
        let mut a = Vec::<PathElement>::new();
        let mut b = Vec::<PathElement>::new();
        original
            .draw(&original.subfont(0, &[]).unwrap(), old, &[], None, &mut a)
            .unwrap();
        subset
            .draw(&subset.subfont(0, &[]).unwrap(), new, &[], None, &mut b)
            .unwrap();
        assert_eq!(a, b);
    }
}

#[test]
fn caller_dependent_calls_fail_retained_mode_and_desubroutinize_correctly() {
    for bytes in [
        include_bytes!("../test-data/fonts/cff1_caller_subr.otf").as_slice(),
        include_bytes!("../test-data/fonts/cff2_caller_subr.otf").as_slice(),
    ] {
        let font = FontRef::new(bytes).unwrap();
        assert!(subset_font(&font, &plan(&font, SubsetFlags::default(), "41,42")).is_err());
        let p = plan(&font, SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE, "41,42");
        let output = subset_font(&font, &p).unwrap();
        let subset = FontRef::new(&output).unwrap();
        let tag = if font.cff2().is_ok() {
            Tag::new(b"CFF2")
        } else {
            Tag::new(b"CFF ")
        };
        let cff = CffFontRef::new(subset.data_for_tag(tag).unwrap().as_bytes(), 0, None).unwrap();
        let original =
            CffFontRef::new(font.data_for_tag(tag).unwrap().as_bytes(), 0, None).unwrap();
        for (old, new) in p
            .old_to_new_glyph_mapping()
            .filter(|(_, g)| g.to_u32() != 0)
        {
            let mut a = Vec::<PathElement>::new();
            let mut b = Vec::<PathElement>::new();
            original
                .draw(&original.subfont(0, &[]).unwrap(), old, &[], None, &mut a)
                .unwrap();
            cff.draw(&cff.subfont(0, &[]).unwrap(), new, &[], None, &mut b)
                .unwrap();
            assert_eq!(a, b);
        }
    }
}

#[test]
fn long_private_blends_and_fd_independent_globals_survive() {
    for filename in ["cff2_long_private_blends.otf", "cff2_shared_global.otf"] {
        let bytes = std::fs::read(format!("test-data/fonts/{filename}")).unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let p = plan(&font, SubsetFlags::default(), "41,42");
        let output = subset_font(&font, &p).unwrap();
        let subset = FontRef::new(&output).unwrap();
        let cff = CffFontRef::new(
            subset.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
            0,
            None,
        )
        .unwrap();
        if filename == "cff2_shared_global.otf" {
            assert_eq!(cff.global_subrs().count(), 1);
            assert_ne!(
                cff.subfont_index(GlyphId::new(1)),
                cff.subfont_index(GlyphId::new(2))
            );
        }
        for (_, gid) in p
            .old_to_new_glyph_mapping()
            .filter(|(_, g)| g.to_u32() != 0)
        {
            let sf = cff.subfont(cff.subfont_index(gid).unwrap(), &[]).unwrap();
            cff.draw(&sf, gid, &[], None, &mut Vec::<PathElement>::new())
                .unwrap();
        }
    }
}
