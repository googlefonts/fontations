use super::*;
use read_fonts::{
    collections::IntSet,
    types::{GlyphId, Tag},
    FontData, FontRead, TableProvider,
};

fn font(wide: bool, version: Version16Dot16) -> Vec<u8> {
    let mut bytes = version.to_be_bytes().to_vec();
    if wide {
        bytes.extend(Uint24::new(70001).to_be_bytes());
    } else {
        bytes.extend(123u16.to_be_bytes());
    }
    if version == Version16Dot16::VERSION_1_0 {
        for value in 1u16..=13 {
            bytes.extend(value.to_be_bytes());
        }
    }
    let mut builder = FontBuilder::new();
    builder.add_raw(if wide { MaxpExtended::TAG } else { Maxp::TAG }, bytes);
    if wide {
        // stands in for the per-glyph tables, see get_font_num_glyphs
        builder.add_raw(Tag::new(b"test"), vec![0u8; 70001]);
    }
    builder.build()
}

fn subset(
    wide: bool,
    version: Version16Dot16,
    count: usize,
    no_hinting: bool,
) -> Result<Vec<u8>, SubsetError> {
    let bytes = font(wide, version);
    let font = FontRef::new(&bytes).unwrap();
    let plan = Plan {
        num_output_glyphs: count,
        subset_flags: if no_hinting {
            SubsetFlags::SUBSET_FLAGS_NO_HINTING
        } else {
            SubsetFlags::SUBSET_FLAGS_DEFAULT
        },
        ..Default::default()
    };
    let mut s = Serializer::new(1024).unwrap();
    s.start_serialize().unwrap();
    if wide {
        font.maxp_extended()
            .unwrap()
            .subset(&plan, &font, &mut s, &mut FontBuilder::new())?;
    } else {
        font.maxp()
            .unwrap()
            .subset(&plan, &font, &mut s, &mut FontBuilder::new())?;
    }
    s.end_serialize();
    assert!(!s.in_error());
    Ok(s.copy_bytes())
}

#[test]
fn maximum_profile_preserves_width_and_hint_fields() {
    for wide in [false, true] {
        for version in [Version16Dot16::VERSION_0_5, Version16Dot16::VERSION_1_0] {
            for no_hinting in [false, true] {
                let count = if wide { 70000 } else { 100 };
                let bytes = subset(wide, version, count, no_hinting).unwrap();
                let table = if wide {
                    read_fonts::tables::maxp::SelectedMaxp::Extended(
                        MaxpExtended::read(FontData::new(&bytes)).unwrap(),
                    )
                } else {
                    read_fonts::tables::maxp::SelectedMaxp::Original(
                        Maxp::read(FontData::new(&bytes)).unwrap(),
                    )
                };
                assert_eq!(table.num_glyphs(), count as u32);
                assert_eq!(table.version(), version);
                if version == Version16Dot16::VERSION_1_0 {
                    assert_eq!(table.max_points(), Some(1));
                    assert_eq!(table.max_component_depth(), Some(13));
                    assert_eq!(table.max_zones(), Some(if no_hinting { 1 } else { 5 }));
                    assert_eq!(
                        table.max_twilight_points(),
                        Some(if no_hinting { 0 } else { 6 })
                    );
                    assert_eq!(
                        table.max_size_of_instructions(),
                        Some(if no_hinting { 0 } else { 11 })
                    );
                }
            }
        }
    }
    assert!(subset(false, Version16Dot16::VERSION_1_0, 65536, false).is_err());
    assert!(subset(true, Version16Dot16::VERSION_1_0, 0x1000000, false).is_err());
}

#[test]
fn planner_selects_extended_maximum_profile() {
    let bytes = font(true, Version16Dot16::VERSION_1_0);
    let source = FontRef::new(&bytes).unwrap();
    let mut builder = FontBuilder::new();
    builder.copy_missing_tables(source);
    builder.add_raw(Maxp::TAG, &[0, 0, 0x50, 0, 0, 123][..]);
    let bytes = builder.build();
    let font = FontRef::new(&bytes).unwrap();
    let plan = Plan::new(
        &[GlyphId::new(70000)].into_iter().collect(),
        &Default::default(),
        &font,
        SubsetFlags::SUBSET_FLAGS_DEFAULT,
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
    );
    assert_eq!(plan.font_num_glyphs, 70001);
    assert_eq!(
        plan.glyph_map.get(&GlyphId::new(70000)),
        Some(&GlyphId::new(1))
    );
    let bytes = crate::subset_font(&font, &plan).unwrap();
    assert_eq!(
        FontRef::new(&bytes)
            .unwrap()
            .maxp_extended()
            .unwrap()
            .num_glyphs(),
        Uint24::new(2)
    );
}

#[test]
fn planner_maps_more_than_65535_retained_glyphs() {
    let bytes = font(true, Version16Dot16::VERSION_1_0);
    let font = FontRef::new(&bytes).unwrap();
    for count in [65535u32, 65536, 70001] {
        let gids = (0..count).map(GlyphId::new).collect();
        let plan = Plan::new(
            &gids,
            &Default::default(),
            &font,
            SubsetFlags::SUBSET_FLAGS_DEFAULT,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        assert_eq!(plan.num_output_glyphs, count as usize);
        assert_eq!(plan.glyph_map.len(), count as usize);
        let last = GlyphId::new(count - 1);
        assert_eq!(plan.glyph_map.get(&last), Some(&last));
    }
}

// A MAXP table can claim 2^24 glyphs in a font of a few bytes. The count is limited to the size
// of the font, and to no less than what a maxp table can claim.
#[test]
fn planner_limits_extended_glyph_count_to_font_size() {
    for padding in [0, 70000, 200000] {
        let mut builder = FontBuilder::new();
        builder.add_raw(MaxpExtended::TAG, &[0, 0, 0x50, 0, 0xFF, 0xFF, 0xFF][..]);
        builder.add_raw(Tag::new(b"test"), vec![0u8; padding]);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let num_glyphs = bytes.len().clamp(0xFFFF, 0xFFFFFF);
        assert_eq!(crate::get_font_num_glyphs(&font), num_glyphs);

        // a request for every glyph retains no more than that
        let plan = Plan::new(
            &IntSet::all(),
            &Default::default(),
            &font,
            SubsetFlags::SUBSET_FLAGS_DEFAULT,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        assert_eq!(plan.num_output_glyphs, num_glyphs);
    }
}

// cmap may map a code point to a glyph beyond the glyph count, which isn't retained. Planning
// must drop that mapping rather than panic.
#[test]
fn planner_ignores_cmap_glyphs_beyond_glyph_count() {
    use skrifa::MetadataProvider;
    for (maxp_tag, maxp, gid) in [
        (Maxp::TAG, &[0, 0, 0x50, 0, 0, 27][..], 100_u32),
        (MaxpExtended::TAG, &[0, 0, 0x50, 0, 1, 0, 0][..], 70000),
    ] {
        // format 12 cmap mapping U+0041 to gid
        let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 10, 0, 0, 0, 12];
        cmap.extend([0, 12, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 0, 1]);
        cmap.extend([0, 0, 0, 0x41, 0, 0, 0, 0x41]);
        cmap.extend(gid.to_be_bytes());
        let mut builder = FontBuilder::new();
        builder.add_raw(maxp_tag, maxp);
        builder.add_raw(Tag::new(b"cmap"), cmap);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        assert_eq!(font.charmap().map(0x41_u32), Some(GlyphId::new(gid)));

        let plan = Plan::new(
            &Default::default(),
            &[0x41].into_iter().collect(),
            &font,
            SubsetFlags::SUBSET_FLAGS_DEFAULT,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        assert!(plan.font_num_glyphs <= gid as usize);
        assert!(!plan.glyphset.contains(GlyphId::new(gid)));
        assert!(plan.unicode_to_new_gid_list.is_empty());
    }
}
