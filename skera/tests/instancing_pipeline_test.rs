//! Instancing shares subset closure, retained rows, and borrowed graph serialization.
use read_fonts::{
    collections::IntSet,
    types::{GlyphId, Tag},
    FontRef, TableProvider,
};
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::FontBuilder;

fn replace(font: &FontRef, tag: Tag, bytes: Vec<u8>) -> Vec<u8> {
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        if record.tag() != tag {
            builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
        }
    }
    builder.add_raw(tag, bytes);
    builder.build()
}

#[test]
fn shared_layout_offsets_do_not_expand_into_an_owned_matrix() {
    let source = FontRef::new(include_bytes!("../test-data/fonts/AdobeVFPrototype.otf")).unwrap();
    // 15,000 lookup offsets share one lookup; that lookup's 15,000 offsets
    // share one SinglePos. Owning each occurrence would expand 225M subtables.
    let count = 15_000u16;
    let mut words = vec![1, 0, 10, 12, 14, 0, 0, count];
    words.extend(std::iter::repeat_n(2 + 2 * count, usize::from(count)));
    words.extend([1, 0, count]);
    words.extend(std::iter::repeat_n(6 + 2 * count, usize::from(count)));
    words.extend([1, 8, 4, 100, 1, 1, 1]); // SinglePos, xAdvance=100, coverage glyph 1.
    let layout = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let bytes = replace(&source, Tag::new(b"GPOS"), layout);
    let font = FontRef::new(&bytes).unwrap();
    let output = instance_font(&font, &parse_axis_limits("wght=900,CNTR=drop").unwrap()).unwrap();
    let font = FontRef::new(&output).unwrap();
    let list = font.gpos().unwrap().lookup_list().unwrap();
    assert_eq!(list.lookup_count(), count);
    assert!(font.data_for_tag(Tag::new(b"GPOS")).unwrap().len() < 100_000);
    let read_fonts::tables::gpos::PositionSubtables::Single(subtables) =
        list.lookups().get(0).unwrap().subtables().unwrap()
    else {
        panic!()
    };
    assert_eq!(subtables.len(), usize::from(count));
}

#[test]
fn unretained_cff2_programs_are_not_interpreted_during_instancing() {
    let source = FontRef::new(include_bytes!("../test-data/fonts/AdobeVFPrototype.otf")).unwrap();
    let data = source.data_for_tag(Tag::new(b"CFF2")).unwrap();
    let mut cff = data.as_bytes().to_vec();
    let parsed = read_fonts::ps::cff::CffFontRef::new(data.as_bytes(), 0, None).unwrap();
    let program = parsed.charstrings().get(2).unwrap();
    let start = program.as_ptr() as usize - data.as_bytes().as_ptr() as usize;
    cff[start] = 29; // callgsubr without an operand in an unretained glyph.
    let bytes = replace(&source, Tag::new(b"CFF2"), cff);
    let font = FontRef::new(&bytes).unwrap();
    let limits = parse_axis_limits("wght=900,CNTR=drop").unwrap();
    assert!(instance_font(&font, &limits).is_err());
    let plan = Plan::new_with_axis_limits(
        &[GlyphId::new(1)].into_iter().collect(),
        &IntSet::empty(),
        &font,
        SubsetFlags::SUBSET_FLAGS_NO_LAYOUT_CLOSURE,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &limits,
    )
    .unwrap();
    let output = subset_font(&font, &plan).unwrap();
    let font = FontRef::new(&output).unwrap();
    assert_eq!(font.maxp().unwrap().num_glyphs(), 2);
    assert!(font.fvar().is_err());
}

#[test]
fn dropping_notdef_outline_still_applies_its_phantom_metrics() {
    use read_fonts::tables::glyf::Glyph;
    let source =
        FontRef::new(include_bytes!("../test-data/fonts/Roboto-Variable.ABC.ttf")).unwrap();
    let glyf = source.glyf().unwrap();
    let loca = source.loca(None).unwrap();
    let count = match loca.get(GlyphId::NOTDEF, &glyf).unwrap().into_glyph() {
        Some(Glyph::Simple(g)) => g.num_points(),
        None => 0,
        _ => panic!(),
    };
    let zeros = |mut count: usize| {
        let mut out = Vec::new();
        while count != 0 {
            let n = count.min(64);
            out.push(0x80 | ((n - 1) as u8));
            count -= n;
        }
        out
    };
    let mut deltas = zeros(count + 1);
    deltas.extend([0, 20]);
    deltas.extend(zeros(2));
    deltas.extend(zeros(count + 4));
    let axes = source.fvar().unwrap().axis_count();
    let glyphs = source.maxp().unwrap().num_glyphs();
    let mut tuple = vec![0, 1];
    tuple.extend((8 + axes * 2).to_be_bytes());
    tuple.extend((deltas.len() as u16).to_be_bytes());
    tuple.extend(0x8000u16.to_be_bytes());
    for i in 0..axes {
        tuple.extend(if i == 0 { 0x4000u16 } else { 0 }.to_be_bytes());
    }
    tuple.extend(deltas);
    let mut gvar = vec![0, 1, 0, 0];
    gvar.extend(axes.to_be_bytes());
    gvar.extend(0u16.to_be_bytes());
    gvar.extend(0u32.to_be_bytes());
    gvar.extend(glyphs.to_be_bytes());
    gvar.extend(1u16.to_be_bytes());
    gvar.extend((20 + 4 * (u32::from(glyphs) + 1)).to_be_bytes());
    gvar.extend(0u32.to_be_bytes());
    for _ in 0..glyphs {
        gvar.extend((tuple.len() as u32).to_be_bytes());
    }
    gvar.extend(tuple);
    let bytes = replace(&source, Tag::new(b"gvar"), gvar);
    let font = FontRef::new(&bytes).unwrap();
    let limits = skera::parse_axis_limits_for_font(&font, "*=drop,wght=900").unwrap();
    let reference = instance_font(&font, &limits).unwrap();
    let reference = FontRef::new(&reference).unwrap();
    let expected = font.hmtx().unwrap().advance(GlyphId::NOTDEF).unwrap() + 20;
    assert_eq!(
        reference.hmtx().unwrap().advance(GlyphId::NOTDEF),
        Some(expected)
    );
    let plan = Plan::new_with_axis_limits(
        &[GlyphId::new(1)].into_iter().collect(),
        &IntSet::empty(),
        &font,
        SubsetFlags::default(),
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &limits,
    )
    .unwrap();
    let output = subset_font(&font, &plan).unwrap();
    let output = FontRef::new(&output).unwrap();
    assert_eq!(
        output.hmtx().unwrap().advance(GlyphId::NOTDEF),
        Some(expected)
    );
    assert!(output
        .loca(None)
        .unwrap()
        .get(GlyphId::NOTDEF, &output.glyf().unwrap())
        .unwrap()
        .into_glyph()
        .is_none());
}

#[test]
fn axis_name_closure_uses_the_retained_metadata() {
    let font = FontRef::new(include_bytes!("../test-data/fonts/RobotoFlex-Variable.ttf")).unwrap();
    let glyphs = [GlyphId::new(36), GlyphId::new(37), GlyphId::new(38)]
        .into_iter()
        .collect();
    let names = (0..=6).map(read_fonts::types::NameId::new).collect();
    let languages = [0x409].into_iter().collect();
    for request in ["*=drop,wght=700", "wght=700"] {
        let limits = skera::parse_axis_limits_for_font(&font, request).unwrap();
        let combined = Plan::new_with_axis_limits(
            &glyphs,
            &IntSet::empty(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &names,
            &languages,
            &limits,
        )
        .unwrap();
        let combined = subset_font(&font, &combined).unwrap();
        let combined = FontRef::new(&combined).unwrap();
        let instance = instance_font(&font, &limits).unwrap();
        let instance = FontRef::new(&instance).unwrap();
        let staged = Plan::new(
            &glyphs,
            &IntSet::empty(),
            &instance,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &names,
            &languages,
        );
        let staged = subset_font(&instance, &staged).unwrap();
        let staged = FontRef::new(&staged).unwrap();
        let replanned = Plan::new(
            &glyphs,
            &IntSet::empty(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &names,
            &languages,
        )
        .with_axis_limits(&font, &limits)
        .unwrap();
        let replanned = subset_font(&font, &replanned).unwrap();
        let replanned = FontRef::new(&replanned).unwrap();
        assert_eq!(
            replanned
                .data_for_tag(Tag::new(b"name"))
                .unwrap()
                .as_bytes(),
            combined.data_for_tag(Tag::new(b"name")).unwrap().as_bytes(),
            "{request}"
        );
        assert_eq!(
            combined.data_for_tag(Tag::new(b"name")).unwrap().as_bytes(),
            staged.data_for_tag(Tag::new(b"name")).unwrap().as_bytes(),
            "{request}"
        );
        if let Ok(hvar) = combined.hvar() {
            // Three letters must not retain the full source region list.
            assert!(
                hvar.item_variation_store()
                    .unwrap()
                    .variation_region_list()
                    .unwrap()
                    .region_count()
                    < 30
            );
        }
    }
}
