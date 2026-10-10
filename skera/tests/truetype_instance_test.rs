//! Compare stored TrueType outlines and metrics with HarfBuzz instances.
use skera::{instance_font, parse_axis_limits};
use write_fonts::read::{tables::glyf::Glyph, types::GlyphId, FontRef, TableProvider};

#[test]
fn full_instances_match_harfbuzz() {
    for (file, request, expected) in [
        (
            "NotoSans-VF.abc.ttf",
            "wght=650,wdth=80,CTGR=drop",
            "tt-instance-noto.ttf",
        ),
        (
            "Roboto-Variable.composite.ttf",
            "wght=725,wdth=90",
            "tt-instance-composite.ttf",
        ),
        ("Muli-ABC.ttf", "wght=700", "tt-instance-cvar.ttf"),
        (
            "RobotoMono.ttf",
            "wght=700",
            "tt-instance-empty-component.ttf",
        ),
    ] {
        let source = std::fs::read(format!("test-data/fonts/{file}")).unwrap();
        let reference = std::fs::read(format!("test-data/fonts/{expected}")).unwrap();
        let bytes = instance_font(
            &FontRef::new(&source).unwrap(),
            &parse_axis_limits(request).unwrap(),
        )
        .unwrap();
        let actual = FontRef::new(&bytes).unwrap();
        let expected = FontRef::new(&reference).unwrap();
        assert!(actual.fvar().is_err());
        assert!(actual.gvar().is_err());
        assert!(actual.cvar().is_err());
        assert_eq!(
            actual.maxp().unwrap().num_glyphs(),
            expected.maxp().unwrap().num_glyphs()
        );
        let (ag, eg) = (actual.glyf().unwrap(), expected.glyf().unwrap());
        let (al, el) = (actual.loca(None).unwrap(), expected.loca(None).unwrap());
        for gid in 0..actual.maxp().unwrap().num_glyphs() {
            let gid = GlyphId::new(gid as u32);
            let a = al.get(gid, &ag).unwrap().into_glyph();
            let e = el.get(gid, &eg).unwrap().into_glyph();
            match (a, e) {
                (None, None) => {}
                (Some(Glyph::Simple(a)), Some(Glyph::Simple(e))) => {
                    assert_eq!(
                        a.points().collect::<Vec<_>>(),
                        e.points().collect::<Vec<_>>(),
                        "{file} {gid:?}"
                    );
                    assert_eq!(a.end_pts_of_contours(), e.end_pts_of_contours());
                    assert_eq!(a.instructions(), e.instructions());
                    assert_eq!(
                        [a.x_min(), a.y_min(), a.x_max(), a.y_max()],
                        [e.x_min(), e.y_min(), e.x_max(), e.y_max()]
                    );
                }
                (Some(Glyph::Composite(a)), Some(Glyph::Composite(e))) => {
                    let aa = a
                        .components()
                        .map(|c| (c.glyph, c.anchor, c.transform))
                        .collect::<Vec<_>>();
                    let ee = e
                        .components()
                        .map(|c| (c.glyph, c.anchor, c.transform))
                        .collect::<Vec<_>>();
                    assert_eq!(aa, ee, "{file} {gid:?}");
                    assert_eq!(a.instructions(), e.instructions());
                    assert_eq!(
                        [a.x_min(), a.y_min(), a.x_max(), a.y_max()],
                        [e.x_min(), e.y_min(), e.x_max(), e.y_max()]
                    );
                }
                _ => panic!("different glyph type: {file} {gid:?}"),
            }
            assert_eq!(
                actual.hmtx().unwrap().advance(gid),
                expected.hmtx().unwrap().advance(gid),
                "{file} {gid:?}"
            );
            assert_eq!(
                actual.hmtx().unwrap().side_bearing(gid),
                expected.hmtx().unwrap().side_bearing(gid),
                "{file} {gid:?}"
            );
        }
        if let Ok(a) = actual.cvt() {
            assert_eq!(a, expected.cvt().unwrap());
        }
    }
}

#[test]
fn fractional_sparse_tuples_match_harfbuzz_float_operation_order() {
    let source = std::fs::read("test-data/fonts/Fraunces.ttf").unwrap();
    let reference = std::fs::read("test-data/fonts/tt-instance-fractional.ttf").unwrap();
    let bytes = instance_font(
        &FontRef::new(&source).unwrap(),
        &parse_axis_limits("opsz=76.5,wght=500,SOFT=50,WONK=0.5").unwrap(),
    )
    .unwrap();
    let actual = FontRef::new(&bytes).unwrap();
    let expected = FontRef::new(&reference).unwrap();
    let (ag, eg) = (actual.glyf().unwrap(), expected.glyf().unwrap());
    let (al, el) = (actual.loca(None).unwrap(), expected.loca(None).unwrap());
    let mut compared = 0;
    for gid in 0..expected.maxp().unwrap().num_glyphs() {
        let gid = GlyphId::new(gid as u32);
        let Some(Glyph::Simple(e)) = el.get(gid, &eg).unwrap().into_glyph() else {
            continue;
        };
        let Some(Glyph::Simple(a)) = al.get(gid, &ag).unwrap().into_glyph() else {
            panic!("{gid:?}")
        };
        assert_eq!(
            a.points().collect::<Vec<_>>(),
            e.points().collect::<Vec<_>>(),
            "{gid:?}"
        );
        compared += 1;
    }
    assert!(compared >= 3);
}
