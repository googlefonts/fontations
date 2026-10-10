//! Compare stored TrueType outlines and metrics with HarfBuzz instances.
use skera::{instance_font, parse_axis_limits};
use write_fonts::from_obj::FromTableRef;
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

fn assert_same_outline_and_metrics(actual: &FontRef, expected: &FontRef) {
    let (ag, eg) = (actual.glyf().unwrap(), expected.glyf().unwrap());
    let (al, el) = (actual.loca(None).unwrap(), expected.loca(None).unwrap());
    assert_eq!(
        actual.maxp().unwrap().num_glyphs(),
        expected.maxp().unwrap().num_glyphs()
    );
    for gid in 0..actual.maxp().unwrap().num_glyphs() {
        let gid = GlyphId::new(gid as u32);
        let a = al
            .get(gid, &ag)
            .unwrap()
            .into_glyph()
            .map(|g| write_fonts::tables::glyf::Glyph::from_table_ref(&g));
        let e = el
            .get(gid, &eg)
            .unwrap()
            .into_glyph()
            .map(|g| write_fonts::tables::glyf::Glyph::from_table_ref(&g));
        assert_eq!(a, e, "{gid:?}");
        assert_eq!(
            actual.hmtx().unwrap().advance(gid),
            expected.hmtx().unwrap().advance(gid),
            "{gid:?}"
        );
        assert_eq!(
            actual.hmtx().unwrap().side_bearing(gid),
            expected.hmtx().unwrap().side_bearing(gid),
            "{gid:?}"
        );
    }
    if let Ok(a) = actual.cvt() {
        assert_eq!(a, expected.cvt().unwrap());
    }
}

#[test]
fn partial_instances_and_second_stage_instances_match_harfbuzz() {
    for (file, request, expected) in [
        ("NotoSans-VF.abc.ttf", "wght=650", "tt-partial-pin.ttf"),
        (
            "NotoSans-VF.abc.ttf",
            "wght=300:550:800,wdth=80:95",
            "tt-partial-range.ttf",
        ),
        ("Muli-ABC.ttf", "wght=500:700:900", "tt-partial-cvar.ttf"),
        (
            "Roboto-Variable.composite.ttf",
            "wght=200:650:850",
            "tt-partial-composite.ttf",
        ),
        ("Recursive-ABC.ttf", "MONO=0.65", "tt-partial-recursive.ttf"),
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
        assert_same_outline_and_metrics(&actual, &expected);
        assert_eq!(
            actual.fvar().unwrap().axes().unwrap(),
            expected.fvar().unwrap().axes().unwrap()
        );
        assert_eq!(
            actual
                .avar()
                .unwrap()
                .axis_segment_maps()
                .iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            expected
                .avar()
                .unwrap()
                .axis_segment_maps()
                .iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        );
        assert_eq!(
            actual.gvar().unwrap().axis_count(),
            actual.fvar().unwrap().axis_count()
        );
        for fraction in [0., 0.25, 0.5, 0.75, 1.] {
            let limits = expected
                .fvar()
                .unwrap()
                .axes()
                .unwrap()
                .iter()
                .map(|a| {
                    let min = a.min_value().to_f64();
                    let max = a.max_value().to_f64();
                    format!("{}={}", a.axis_tag(), min + (max - min) * fraction)
                })
                .collect::<Vec<_>>()
                .join(",");
            let limits = parse_axis_limits(&limits).unwrap();
            let a = instance_font(&actual, &limits).unwrap();
            let e = instance_font(&expected, &limits).unwrap();
            assert_same_outline_and_metrics(&FontRef::new(&a).unwrap(), &FontRef::new(&e).unwrap());
        }
    }
}
