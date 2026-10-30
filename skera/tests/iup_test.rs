//! Residual gvar tuples match HarfBuzz c82300aefb with --optimize.
//! Reference 5 also includes the serialization retry fix from HB 8f62264c5.
use skera::{instance_font, instance_font_with_flags, parse_axis_limits, SubsetFlags};
use write_fonts::read::{
    tables::{gvar::Gvar, variations::Tuple},
    types::GlyphId,
    FontData, FontRead, FontRef, TableProvider,
};

type StoredTuple = (
    Vec<i16>,
    Option<Vec<i16>>,
    Option<Vec<i16>>,
    Vec<(u16, i32, i32)>,
);

fn stored_tuples(gvar: &Gvar, gid: GlyphId) -> Vec<StoredTuple> {
    let coords = |t: Tuple| (0..t.len()).map(|i| t.get(i).unwrap().to_bits()).collect();
    let mut result = Vec::new();
    if let Some(data) = gvar.glyph_variation_data(gid).unwrap() {
        for t in data.tuples() {
            result.push((
                coords(t.peak()),
                t.intermediate_start().map(coords),
                t.intermediate_end().map(coords),
                t.deltas()
                    .map(|d| (d.position, d.x_delta, d.y_delta))
                    .collect(),
            ));
        }
    }
    result.sort();
    result
}

#[test]
fn optimized_sparse_points_and_deltas_match_harfbuzz() {
    let mut reduced = false;
    for (n, file, request) in [
        (0, "NotoSans-VF.abc.ttf", "wght=650"),
        (1, "NotoSans-VF.abc.ttf", "wght=300:550:800,wdth=80:95"),
        (2, "Roboto-Variable.composite.ttf", "wght=200:650:850"),
        (3, "Recursive-ABC.ttf", "MONO=0.65"),
        (4, "Muli-ABC.ttf", "wght=500:700:900"),
        (5, "RobotoFlex-Variable.ABC.ttf", "wght=280:640:865"),
    ] {
        let source = std::fs::read(format!("test-data/fonts/{file}")).unwrap();
        let font = FontRef::new(&source).unwrap();
        let limits = parse_axis_limits(request).unwrap();
        let optimized = instance_font_with_flags(
            &font,
            &limits,
            SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS,
        )
        .unwrap();
        let dense = instance_font(&font, &limits).unwrap();
        let actual = FontRef::new(&optimized).unwrap();
        let dense = FontRef::new(&dense).unwrap();
        let reference = std::fs::read(format!("test-data/expected/tt-iup/{n}-gvar.bin")).unwrap();
        let expected = Gvar::read(FontData::new(&reference)).unwrap();
        let gvar = actual.gvar().unwrap();
        assert_eq!(gvar.axis_count(), expected.axis_count());
        assert_eq!(gvar.glyph_count(), expected.glyph_count());
        for gid in 0..gvar.glyph_count() {
            let gid = GlyphId::new(gid as u32);
            assert_eq!(
                stored_tuples(&gvar, gid),
                stored_tuples(&expected, gid),
                "{file} {gid:?}"
            );
        }
        let tag = write_fonts::types::Tag::new(b"gvar");
        let actual_size = actual.data_for_tag(tag).unwrap().len();
        let dense_size = dense.data_for_tag(tag).unwrap().len();
        assert!(
            actual_size <= dense_size,
            "{file}: {actual_size} > {dense_size}"
        );
        reduced |= actual_size < dense_size;
        for tag in [b"glyf", b"loca", b"hmtx", b"vmtx", b"cvar", b"cvt "] {
            let tag = write_fonts::types::Tag::new(tag);
            assert_eq!(
                actual.data_for_tag(tag).map(|d| d.as_bytes()),
                dense.data_for_tag(tag).map(|d| d.as_bytes()),
                "{file} {tag}"
            );
        }
    }
    assert!(reduced);
}

#[test]
fn large_contours_keep_variations_when_optimization_is_bounded() {
    let source = std::fs::read("test-data/fonts/iup-large-contour.ttf").unwrap();
    let font = FontRef::new(&source).unwrap();
    let partial = instance_font_with_flags(
        &font,
        &parse_axis_limits("wght=400:900").unwrap(),
        SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS,
    )
    .unwrap();
    let partial = FontRef::new(&partial).unwrap();
    let gvar = partial.gvar().unwrap();
    let tuples = stored_tuples(&gvar, GlyphId::new(1));
    assert_eq!(tuples.len(), 1);
    assert!(tuples[0].3.len() >= 600);
    for weight in [400, 650, 900] {
        let limits = parse_axis_limits(&format!("wght={weight}")).unwrap();
        let direct = instance_font(&font, &limits).unwrap();
        let composed = instance_font(&partial, &limits).unwrap();
        let direct = FontRef::new(&direct).unwrap();
        let composed = FontRef::new(&composed).unwrap();
        for tag in [b"glyf", b"loca", b"hmtx"] {
            let tag = write_fonts::types::Tag::new(tag);
            assert_eq!(
                direct.data_for_tag(tag).map(|d| d.as_bytes()),
                composed.data_for_tag(tag).map(|d| d.as_bytes())
            );
        }
    }
}
