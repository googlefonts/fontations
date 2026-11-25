use super::*;
use crate::{subset_font, SubsetFlags};
use font_test_data::cmap::{font_with_cmaps, format12, format14, format15, table};
use read_fonts::{
    tables::cmap::MapVariant,
    types::{NameId, Tag},
    FontData, FontRead, TableProvider,
};
use skrifa::MetadataProvider;

fn plan(font: &FontRef, unicodes: &[u32], retain_gids: bool) -> Plan {
    Plan::new(
        &IntSet::empty(),
        &IntSet::from_iter(unicodes.iter().copied()),
        font,
        if retain_gids {
            SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
        } else {
            SubsetFlags::SUBSET_FLAGS_DEFAULT
        },
        &IntSet::<Tag>::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::<NameId>::empty(),
        &IntSet::empty(),
    )
}

fn uvs(wide: bool, defaults: &[u32], variants: &[(u32, u32)]) -> Vec<u8> {
    if wide {
        format15(0xe0100, defaults, variants)
    } else {
        format14(
            0xe0100,
            defaults,
            &variants
                .iter()
                .map(|(cp, gid)| (*cp, *gid as u16))
                .collect::<Vec<_>>(),
        )
    }
}

#[test]
fn dmap_subset_preserves_nominal_overrides_and_zero_fallback() {
    let nominal = format12(&[(65, 10), (66, 11), (67, 12), (69, 13)]);
    let overrides = format12(&[(65, 40), (66, 0), (68, 41), (70, 42)]);
    let cmap = table(&[(0, 4, &nominal)]);
    let dmap = table(&[(0, 4, &overrides)]);
    let data = font_with_cmaps(Some(&cmap), Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    for retain_gids in [false, true] {
        let plan = plan(&font, &[65, 66, 67, 68], retain_gids);
        let output = subset_font(&font, &plan).unwrap();
        let output = FontRef::new(&output).unwrap();
        for cp in 65..=68u32 {
            let old_gid = font.charmap().map(cp).unwrap();
            assert_eq!(
                output.charmap().map(cp),
                plan.glyph_map.get(&old_gid).copied()
            );
        }
        assert_eq!(output.charmap().map(69u32), None);
        assert_eq!(output.charmap().map(70u32), None);
        let delta = output.dmap().unwrap().as_cmap();
        assert_eq!(delta.map_codepoint(66u32), None);
        assert_eq!(delta.map_codepoint(67u32), None);
        assert_eq!(delta.encoding_records().len(), 1);
        assert_eq!(
            output.maxp().unwrap().num_glyphs() as usize,
            plan.num_output_glyphs
        );
    }
}

#[test]
fn cmap_subtables_keep_their_own_mappings_beside_dmap() {
    let nominal = format12(&[(65, 10), (66, 11)]);
    let overrides = format12(&[(65, 40)]);
    let cmap = table(&[(0, 4, &nominal)]);
    let dmap = table(&[(0, 4, &overrides)]);
    let data = font_with_cmaps(Some(&cmap), Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    for retain_gids in [false, true] {
        let plan = plan(&font, &[65, 66], retain_gids);
        let output = subset_font(&font, &plan).unwrap();
        let output = FontRef::new(&output).unwrap();
        let cmap = output.cmap().unwrap();
        let subtable = cmap.subtable(0).unwrap();
        // DMAP's glyph for A must not leak into cmap.
        assert_eq!(
            subtable.map_codepoint(65u32),
            plan.glyph_map.get(&GlyphId::new(10)).copied()
        );
        assert_eq!(
            subtable.map_codepoint(66u32),
            plan.glyph_map.get(&GlyphId::new(11)).copied()
        );
    }
}

#[test]
fn dmap_subset_closes_variants_and_preserves_default_and_zero_fallback() {
    let nominal = format12(&[(65, 10), (66, 11), (67, 12), (68, 13)]);
    let overrides = format12(&[(65, 40), (66, 41)]);
    for cmap_wide in [false, true] {
        for dmap_wide in [false, true] {
            let cmap_uvs = uvs(cmap_wide, &[], &[(65, 20), (66, 21), (67, 22), (68, 23)]);
            let dmap_uvs = uvs(dmap_wide, &[66], &[(65, 50), (67, 0)]);
            let cmap = table(&[(0, 4, &nominal), (0, 5, &cmap_uvs)]);
            let dmap = table(&[(0, 4, &overrides), (0, 5, &dmap_uvs)]);
            let data = font_with_cmaps(Some(&cmap), Some(&dmap));
            let font = FontRef::new(&data).unwrap();
            for retain_gids in [false, true] {
                let plan = plan(&font, &[65, 66, 67, 68, 0xe0100], retain_gids);
                assert!(plan.glyph_map.contains_key(&GlyphId::new(50)));
                assert!(plan.glyph_map.contains_key(&GlyphId::new(22)));
                let output = subset_font(&font, &plan).unwrap();
                let output = FontRef::new(&output).unwrap();
                for cp in 65..=68u32 {
                    let expected = match font.charmap().map_variant(cp, 0xe0100u32).unwrap() {
                        MapVariant::UseDefault => MapVariant::UseDefault,
                        MapVariant::Variant(gid) => MapVariant::Variant(plan.glyph_map[&gid]),
                    };
                    assert_eq!(output.charmap().map_variant(cp, 0xe0100u32), Some(expected));
                }
                let (_, delta_uvs) = output.dmap().unwrap().as_cmap().uvs_subtable().unwrap();
                assert_eq!(
                    delta_uvs.map_variant(67u32, 0xe0100u32),
                    Some(MapVariant::Variant(GlyphId::NOTDEF))
                );
                assert_eq!(
                    output.charmap().map(66u32),
                    Some(plan.glyph_map[&GlyphId::new(41)])
                );
                assert_eq!(delta_uvs.map_variant(68u32, 0xe0100u32), None);
            }
        }
    }
}

#[test]
fn dmap_subset_supports_uvs_only_and_dmap_only_fonts_and_drops_empty_tables() {
    let nominal = format12(&[(65, 10)]);
    for wide in [false, true] {
        let variant = uvs(wide, &[65], &[(66, 30)]);
        let cmap = table(&[(0, 4, &nominal)]);
        let dmap = table(&[(0, 5, &variant)]);
        let data = font_with_cmaps(Some(&cmap), Some(&dmap));
        let font = FontRef::new(&data).unwrap();
        let output = subset_font(&font, &plan(&font, &[65, 0xe0100], false)).unwrap();
        let output = FontRef::new(&output).unwrap();
        assert_eq!(output.dmap().unwrap().num_tables(), 1);
        assert_eq!(
            output.charmap().map_variant(65u32, 0xe0100u32),
            Some(MapVariant::UseDefault)
        );
        let output = subset_font(&font, &plan(&font, &[65], false)).unwrap();
        assert!(FontRef::new(&output).unwrap().dmap().is_err());

        let nominal = format12(&[(65, 10), (66, 11)]);
        let dmap = table(&[(0, 4, &nominal), (0, 5, &variant)]);
        let data = font_with_cmaps(None, Some(&dmap));
        let font = FontRef::new(&data).unwrap();
        let plan = plan(&font, &[65, 66, 0xe0100], false);
        assert!(plan.glyph_map.contains_key(&GlyphId::new(30)));
        let output = subset_font(&font, &plan).unwrap();
        let output = FontRef::new(&output).unwrap();
        assert!(output.cmap().is_err());
        assert_eq!(output.charmap().map(65u32), Some(GlyphId::new(1)));
        assert_eq!(
            output.charmap().map_variant(66u32, 0xe0100u32),
            Some(MapVariant::Variant(GlyphId::new(3)))
        );
    }
}

#[test]
fn dmap_subset_keeps_format13_constant_groups_and_zero_language() {
    let mut constant = format12(&[(65, 40)]);
    constant[..2].copy_from_slice(&13u16.to_be_bytes());
    constant[8..12].copy_from_slice(&17u32.to_be_bytes());
    constant[20..24].copy_from_slice(&69u32.to_be_bytes());
    let dmap = table(&[(0, 6, &constant)]);
    let data = font_with_cmaps(None, Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    let output = subset_font(&font, &plan(&font, &[65, 66, 68], false)).unwrap();
    let output = FontRef::new(&output).unwrap();
    let delta = output.dmap().unwrap().as_cmap();
    let subtable = delta.encoding_records()[0]
        .subtable(delta.offset_data())
        .unwrap();
    let CmapSubtable::Format13(subtable) = subtable else {
        panic!("format 13 lowered")
    };
    assert_eq!(subtable.language(), 0);
    assert_eq!(subtable.num_groups(), 2);
    for cp in [65u32, 66, 68] {
        assert_eq!(output.charmap().map(cp), Some(GlyphId::new(1)));
    }
    assert_eq!(output.charmap().map(67u32), None);
}

#[test]
fn dmap_subset_preserves_format4_and_zeroes_its_language() {
    let format4: Vec<_> = [
        4u16,
        32,
        17,
        4,
        4,
        1,
        0,
        66,
        0xffff,
        0,
        65,
        0xffff,
        40u16.wrapping_sub(65),
        1,
        0,
        0,
    ]
    .into_iter()
    .flat_map(u16::to_be_bytes)
    .collect();
    let dmap = table(&[(0, 3, &format4)]);
    let data = font_with_cmaps(None, Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    let output = subset_font(&font, &plan(&font, &[66], false)).unwrap();
    let output = FontRef::new(&output).unwrap();
    let dmap = output.dmap().unwrap().as_cmap();
    let subtable = dmap.encoding_records()[0]
        .subtable(dmap.offset_data())
        .unwrap();
    assert_eq!(subtable.format(), 4);
    assert_eq!(subtable.language(), 0);
    assert_eq!(output.charmap().map(65u32), None);
    assert_eq!(output.charmap().map(66u32), Some(GlyphId::new(1)));
}

#[test]
fn dmap_subset_rejects_truncated_records_and_wide_format4_glyphs() {
    let mut dmap = table(&[(0, 4, &format12(&[(65, 10)]))]);
    dmap[2..4].copy_from_slice(&2u16.to_be_bytes());
    dmap.truncate(12);
    let data = font_with_cmaps(None, Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    assert!(subset_font(&font, &plan(&font, &[65], false)).is_err());

    let format4: Vec<_> = [
        4u16,
        32,
        0,
        4,
        4,
        1,
        0,
        65,
        0xffff,
        0,
        65,
        0xffff,
        1u16.wrapping_sub(65),
        1,
        0,
        0,
    ]
    .into_iter()
    .flat_map(u16::to_be_bytes)
    .collect();
    let dmap = table(&[(0, 3, &format4)]);
    let source = Dmap::read(FontData::new(&dmap)).unwrap();
    let mut plan = Plan::default();
    plan.unicode_to_new_gid_list.push((65, GlyphId::new(70000)));
    plan.glyph_map.insert(GlyphId::new(1), GlyphId::new(70000));
    let mut s = Serializer::new(4096).unwrap();
    s.start_serialize().unwrap();
    assert_eq!(
        serialize_dmap(&source.as_cmap(), &mut s, &plan),
        Err(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW)
    );
}

#[test]
fn dmap_subset_does_not_activate_lower_priority_mappings() {
    let nominal = format12(&[(65, 10), (66, 11), (67, 12)]);
    let lower = format12(&[(65, 11)]);
    let preferred = format12(&[(68, 40)]);
    let lower_uvs = format14(0xe0100, &[], &[(65, 11)]);
    let preferred_uvs = format15(0xe0100, &[], &[(68, 41)]);
    let cmap_uvs = format14(0xe0100, &[], &[(65, 12)]);
    let cmap = table(&[(0, 4, &nominal), (0, 5, &cmap_uvs)]);
    let dmap = table(&[
        (0, 3, &lower),
        (0, 4, &preferred),
        (0, 5, &lower_uvs),
        (0, 5, &preferred_uvs),
    ]);
    let data = font_with_cmaps(Some(&cmap), Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    assert_eq!(font.charmap().map(65u32), Some(GlyphId::new(10)));
    assert_eq!(
        font.charmap().map_variant(65u32, 0xe0100u32),
        Some(MapVariant::Variant(GlyphId::new(12)))
    );
    let plan = plan(&font, &[65, 66, 67, 0xe0100], false);
    let output = subset_font(&font, &plan).unwrap();
    let output = FontRef::new(&output).unwrap();
    assert_eq!(
        output.charmap().map(65u32),
        Some(plan.glyph_map[&GlyphId::new(10)])
    );
    assert_eq!(
        output.charmap().map_variant(65u32, 0xe0100u32),
        Some(MapVariant::Variant(plan.glyph_map[&GlyphId::new(12)]))
    );
}

#[test]
fn dmap_subset_keeps_wide_nominal_and_variant_glyph_ids() {
    let mut nominal = format12(&[(65, 65536)]);
    nominal[8..12].copy_from_slice(&17u32.to_be_bytes());
    let variant = format15(0xe0100, &[], &[(65, 70000)]);
    let dmap = table(&[(0, 4, &nominal), (0, 5, &variant)]);
    let source = Dmap::read(FontData::new(&dmap)).unwrap();
    for (nominal_gid, variant_gid) in [(1, 2), (65536, 70000)] {
        let mut plan = Plan::default();
        plan.unicodes.extend([65, 0xe0100]);
        plan.unicode_to_new_gid_list
            .push((65, GlyphId::new(nominal_gid)));
        plan.glyph_map.extend([
            (GlyphId::new(65536), GlyphId::new(nominal_gid)),
            (GlyphId::new(70000), GlyphId::new(variant_gid)),
        ]);
        let mut s = Serializer::new(4096).unwrap();
        s.start_serialize().unwrap();
        serialize_dmap(&source.as_cmap(), &mut s, &plan).unwrap();
        s.end_serialize();
        let output = s.copy_bytes();
        let output = Dmap::read(FontData::new(&output)).unwrap().as_cmap();
        assert_eq!(output.map_codepoint(65u32), Some(GlyphId::new(nominal_gid)));
        assert_eq!(
            output.encoding_records()[0]
                .subtable(output.offset_data())
                .unwrap()
                .language(),
            0
        );
        let (_, variant) = output.uvs_subtable().unwrap();
        assert_eq!(variant.format(), 15);
        assert_eq!(
            variant.map_variant(65u32, 0xe0100u32),
            Some(MapVariant::Variant(GlyphId::new(variant_gid)))
        );
    }
}

#[test]
fn dmap_subset_rejects_bad_uvs_offsets_and_truncated_nominal_groups() {
    let nominal = table(&[(0, 4, &format12(&[(65, 10)]))]);
    for format in [12u16, 13] {
        let mut truncated = format12(&[(65, 40)]);
        truncated[..2].copy_from_slice(&format.to_be_bytes());
        truncated[12..16].copy_from_slice(&2u32.to_be_bytes());
        let dmap = table(&[(0, 4, &truncated)]);
        let data = font_with_cmaps(Some(&nominal), Some(&dmap));
        let font = FontRef::new(&data).unwrap();
        assert!(subset_font(&font, &plan(&font, &[65], false)).is_err());
    }
    let mut dmap = table(&[(0, 5, &format15(0xe0100, &[65], &[]))]);
    dmap[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    let data = font_with_cmaps(Some(&nominal), Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    assert!(subset_font(&font, &plan(&font, &[65, 0xe0100], false)).is_err());
}

#[test]
fn repeated_encoding_records_are_subsetted_once() {
    let nominal = format12(&[(65, 10), (66, 11)]);
    let overrides = format12(&[(65, 40)]);
    let uvs14 = uvs(false, &[], &[(65, 20)]);
    let uvs15 = uvs(true, &[], &[(65, 20)]);
    let cmap = table(&[
        (0, 4, &nominal),
        (0, 4, &nominal),
        // The records of variation selector subtables are all kept.
        (0, 5, &uvs14),
        (0, 5, &uvs14),
        (0, 5, &uvs15),
        (3, 10, &nominal),
        (3, 10, &nominal),
        (3, 10, &nominal),
    ]);
    let dmap = table(&[(0, 4, &overrides), (0, 4, &overrides), (3, 10, &overrides)]);
    let data = font_with_cmaps(Some(&cmap), Some(&dmap));
    let font = FontRef::new(&data).unwrap();
    let plan = plan(&font, &[65, 66, 0xe0100], false);
    let output = subset_font(&font, &plan).unwrap();
    let output = FontRef::new(&output).unwrap();
    let ids = |cmap: Cmap| -> Vec<_> {
        cmap.encoding_records()
            .iter()
            .map(|r| (r.platform_id() as u16, r.encoding_id()))
            .collect()
    };
    assert_eq!(
        ids(output.cmap().unwrap()),
        [(0, 4), (0, 5), (0, 5), (0, 5), (3, 10)]
    );
    assert_eq!(ids(output.dmap().unwrap().as_cmap()), [(0, 4), (3, 10)]);
    for (cp, old_gid) in [(65u32, 40), (66, 11)] {
        assert_eq!(
            output.charmap().map(cp),
            Some(plan.glyph_map[&GlyphId::new(old_gid)])
        );
    }
}

/// A format 4 subtable with one segment for each mapping, which must be sorted by codepoint.
fn format4(language: u16, mappings: &[(u16, u16)]) -> Vec<u8> {
    let segments: Vec<_> = mappings.iter().copied().chain([(0xffff, 0)]).collect();
    let seg_count = segments.len() as u16;
    let entry_selector = seg_count.ilog2() as u16;
    let search_range = 2 << entry_selector;
    let header = [
        4,
        16 + 8 * seg_count,
        language,
        2 * seg_count,
        search_range,
        entry_selector,
        2 * seg_count - search_range,
    ];
    let codes = segments.iter().map(|(cp, _)| *cp);
    let deltas = segments.iter().map(|(cp, gid)| gid.wrapping_sub(*cp));
    header
        .into_iter()
        .chain(codes.clone()) // endCode
        .chain([0]) // reservedPad
        .chain(codes) // startCode
        .chain(deltas)
        .chain(segments.iter().map(|_| 0)) // idRangeOffset
        .flat_map(u16::to_be_bytes)
        .collect()
}

#[test]
fn format12_is_only_dropped_for_a_record_that_is_subset() {
    let records = |font: &FontRef, unicodes: &[u32]| -> Vec<_> {
        let output = subset_font(font, &plan(font, unicodes, false)).unwrap();
        let output = FontRef::new(&output).unwrap();
        let cmap = output.cmap().unwrap();
        cmap.encoding_records()
            .iter()
            .map(|r| {
                let subtable = r.subtable(cmap.offset_data()).unwrap();
                let mapped: Vec<_> = (65..=66u32)
                    .filter(|cp| subtable.map_codepoint(*cp).is_some())
                    .collect();
                (r.platform_id() as u16, r.encoding_id(), mapped)
            })
            .collect()
    };
    let bmp_a = format4(1, &[(65, 10)]);
    let bmp_ab = format4(0, &[(65, 10), (66, 11)]);
    let full = format12(&[(65, 10), (66, 11)]);

    // The BMP record has the language and the codepoints of the format 12 subtable, which is
    // left out.
    let cmap = table(&[(3, 1, &bmp_ab), (3, 10, &full)]);
    let data = font_with_cmaps(Some(&cmap), None);
    let font = FontRef::new(&data).unwrap();
    assert_eq!(records(&font, &[65, 66]), [(3, 1, vec![65, 66])]);

    // Only the first of two records (3, 1) is subset, and it has neither. The format 12
    // subtable is not left out for the second one, which alone maps 'B' with it.
    let cmap = table(&[(3, 1, &bmp_a), (3, 1, &bmp_ab), (3, 10, &full)]);
    let data = font_with_cmaps(Some(&cmap), None);
    let font = FontRef::new(&data).unwrap();
    assert_eq!(
        records(&font, &[65, 66]),
        [(3, 1, vec![65]), (3, 10, vec![65, 66])]
    );
}
