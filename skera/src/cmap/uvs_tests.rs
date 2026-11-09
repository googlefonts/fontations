use super::*;
use crate::{subset_font, SubsetFlags};
use font_test_data::cmap::{font_with_cmaps, format12, format14, format15, table};
use read_fonts::{
    tables::cmap::MapVariant,
    types::{NameId, Tag},
    FontData, FontRead, TableProvider,
};

fn serialize_variant(source: &CmapSubtable, plan: &Plan) -> Result<Vec<u8>, SerializeErrorFlags> {
    let mut s = Serializer::new(4096).unwrap();
    s.start_serialize().unwrap();
    source.serialize(&mut s, plan, &[])?;
    s.end_serialize();
    Ok(s.copy_bytes())
}

fn wide_plan() -> Plan {
    let mut plan = Plan::default();
    plan.unicodes.extend([65, 67, 68, 69, 70, 0xe0100]);
    plan.glyphs_requested.insert(GlyphId::new(7));
    plan.glyph_map.extend([
        (GlyphId::new(65536), GlyphId::new(3)),
        (GlyphId::new(70000), GlyphId::new(70000)),
        (GlyphId::new(0xffffff), GlyphId::new(0xffffff)),
        (GlyphId::new(7), GlyphId::new(4)),
    ]);
    plan
}

#[test]
fn cmap15_subset_preserves_wide_format_and_prunes_uvs_records() {
    let data = format15(
        0xe0100,
        &[65, 66, 67],
        &[(68, 65536), (69, 70000), (70, 0xffffff), (71, 7), (72, 8)],
    );
    let source = CmapSubtable::read(FontData::new(&data)).unwrap();
    let output = serialize_variant(&source, &wide_plan()).unwrap();
    let output = Cmap15::read(FontData::new(&output)).unwrap();
    assert_eq!(output.format(), 15);
    assert_eq!(output.length() as usize, output.offset_data().len());
    assert_eq!(output.num_var_selector_records(), 1);
    let output = read_fonts::tables::cmap::UvsSubtable::from(output);
    for (cp, expected) in [
        (65, Some(MapVariant::UseDefault)),
        (66, None),
        (67, Some(MapVariant::UseDefault)),
        (68, Some(MapVariant::Variant(GlyphId::new(3)))),
        (69, Some(MapVariant::Variant(GlyphId::new(70000)))),
        (70, Some(MapVariant::Variant(GlyphId::new(0xffffff)))),
        (71, Some(MapVariant::Variant(GlyphId::new(4)))),
        (72, None),
    ] {
        assert_eq!(output.map_variant(cp as u32, 0xe0100u32), expected);
    }
    assert_eq!(output.iter().count(), 6);
}

#[test]
fn uvs_subset_omits_empty_selectors_and_missing_glyphs() {
    for format in [14, 15] {
        let data = if format == 14 {
            format14(0xe0100, &[], &[(68, 25)])
        } else {
            format15(0xe0100, &[], &[(68, 70000)])
        };
        let source = CmapSubtable::read(FontData::new(&data)).unwrap();
        assert!(serialize_variant(&source, &Plan::default())
            .unwrap()
            .is_empty());
        let mut plan = Plan::default();
        plan.unicodes.extend([68, 0xe0100]);
        assert!(serialize_variant(&source, &plan).unwrap().is_empty());
    }
}

#[test]
fn uvs_subset_rejects_glyph_and_array_overflow() {
    for (format, old_gid, max_gid) in [(14, 25, 0xffff), (15, 70000, 0xffffff)] {
        let data = if format == 14 {
            format14(0xe0100, &[], &[(68, old_gid as u16)])
        } else {
            format15(0xe0100, &[], &[(68, old_gid)])
        };
        let source = CmapSubtable::read(FontData::new(&data)).unwrap();
        let mut plan = Plan::default();
        plan.unicodes.extend([68, 0xe0100]);
        plan.glyph_map
            .insert(GlyphId::new(old_gid), GlyphId::new(max_gid));
        assert!(!serialize_variant(&source, &plan).unwrap().is_empty());
        plan.glyph_map
            .insert(GlyphId::new(old_gid), GlyphId::new(max_gid + 1));
        assert_eq!(
            serialize_variant(&source, &plan),
            Err(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW)
        );

        let mut truncated = data.clone();
        truncated[6..10].copy_from_slice(&2u32.to_be_bytes());
        let source = CmapSubtable::read(FontData::new(&truncated)).unwrap();
        assert_eq!(
            serialize_variant(&source, &plan),
            Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)
        );
        let mut truncated = data;
        truncated.pop();
        let source = CmapSubtable::read(FontData::new(&truncated)).unwrap();
        assert_eq!(
            serialize_variant(&source, &plan),
            Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)
        );
    }
}

/// Repeats the single variation selector record of `data` (built by `format14` or
/// `format15`) `num_records` times, with increasing selectors and all records naming
/// the same default and non-default UVS tables.
fn share_uvs_tables(data: &[u8], num_records: u32) -> Vec<u8> {
    let mut shared = data[..10].to_vec();
    shared[6..10].copy_from_slice(&num_records.to_be_bytes());
    for i in 0..num_records {
        let mut record = data[10..21].to_vec();
        let selector = u32::from_be_bytes([0, record[0], record[1], record[2]]) + i;
        record[..3].copy_from_slice(&selector.to_be_bytes()[1..]);
        for offset in [3..7, 7..11] {
            let value = u32::from_be_bytes(record[offset.clone()].try_into().unwrap());
            if value != 0 {
                let value = value + (num_records - 1) * 11;
                record[offset].copy_from_slice(&value.to_be_bytes());
            }
        }
        shared.extend_from_slice(&record);
    }
    shared.extend_from_slice(&data[21..]);
    let length = shared.len() as u32;
    shared[2..6].copy_from_slice(&length.to_be_bytes());
    shared
}

/// A format 14 or 15 subtable whose `num_records` records name the same default UVS
/// table of 500 ranges and non-default UVS table of 100 mappings to glyph 25.
fn shared_uvs_subtable(format: u16, num_records: u32) -> Vec<u8> {
    let defaults: Vec<u32> = (0..500).map(|i| 0x4e00 + i * 2).collect();
    let variants: Vec<(u32, u16)> = (0..100).map(|i| (0x6000 + i, 25)).collect();
    let data = if format == 14 {
        format14(0xe0100, &defaults, &variants)
    } else {
        let variants: Vec<(u32, u32)> = variants.iter().map(|(cp, g)| (*cp, *g as u32)).collect();
        format15(0xe0100, &defaults, &variants)
    };
    share_uvs_tables(&data, num_records)
}

/// Whether the UVS tables that the records of `subtable` with a selector in
/// `unicodes` name fit the budget of a cmap table of the subtable's size.
fn uvs_ops_fit(subtable: &[u8], unicodes: &IntSet<u32>) -> bool {
    let source = CmapSubtable::read(FontData::new(subtable)).unwrap();
    let mut budget = crate::layout::OpsBudget::new(FontData::new(subtable));
    count_uvs_subtable(&source, unicodes, &mut budget.walk())
}

#[test]
fn uvs_subset_keeps_shared_tables_shared() {
    const NUM_RECORDS: u32 = 10;
    let mut plan = Plan::default();
    plan.unicodes.extend((0..500).map(|i| 0x4e00 + i * 2));
    plan.unicodes.extend(0x6000..0x6000 + 100);
    plan.unicodes.extend(0xe0100..0xe0100 + NUM_RECORDS);
    plan.glyph_map.insert(GlyphId::new(25), GlyphId::new(25));
    for format in [14, 15] {
        let shared = shared_uvs_subtable(format, NUM_RECORDS);
        let source = CmapSubtable::read(FontData::new(&shared)).unwrap();
        let mut s = Serializer::new(1024 * 1024).unwrap();
        s.start_serialize().unwrap();
        source.serialize(&mut s, &plan, &[]).unwrap();
        s.end_serialize();
        // every record still names the one copy of each table
        let output = s.copy_bytes();
        assert_eq!(output, shared);
        let output = match CmapSubtable::read(FontData::new(&output)).unwrap() {
            CmapSubtable::Format14(table) => read_fonts::tables::cmap::UvsSubtable::from(table),
            CmapSubtable::Format15(table) => read_fonts::tables::cmap::UvsSubtable::from(table),
            _ => unreachable!(),
        };
        for selector in [0xe0100, 0xe0100 + NUM_RECORDS - 1] {
            assert_eq!(
                output.map_variant(0x4e00u32, selector),
                Some(MapVariant::UseDefault)
            );
            assert_eq!(
                output.map_variant(0x6000u32, selector),
                Some(MapVariant::Variant(GlyphId::new(25)))
            );
        }
    }
}

#[test]
fn uvs_tables_count_once_for_each_retained_record() {
    for format in [14, 15] {
        // 2 records sharing 2.5 KB of tables fit the budget; 200 don't
        let selectors = |count: u32| IntSet::from_iter(0xe0100..0xe0100 + count);
        assert!(uvs_ops_fit(&shared_uvs_subtable(format, 2), &selectors(2)));
        let shared = shared_uvs_subtable(format, 200);
        assert!(!uvs_ops_fit(&shared, &selectors(200)));
        // records whose selector isn't retained count nothing
        assert!(uvs_ops_fit(&shared, &selectors(2)));
        assert!(uvs_ops_fit(&shared, &IntSet::empty()));
    }
}

#[test]
fn uvs_overlapping_tables_count_for_each_record() {
    const NUM_RECORDS: u32 = 1000;
    // both a default UVS table with 320 ranges and the range U+0001..=U+0041
    const WORD: u32 = 0x0000_0140;
    for format in [14u16, 15] {
        let mut data = vec![0; 10];
        data[..2].copy_from_slice(&format.to_be_bytes());
        data[6..10].copy_from_slice(&NUM_RECORDS.to_be_bytes());
        // every record names a different table, 4 bytes into the previous one
        for i in 0..NUM_RECORDS {
            data.extend_from_slice(&(0xe0100 + i).to_be_bytes()[1..]);
            data.extend_from_slice(&(10 + NUM_RECORDS * 11 + i * 4).to_be_bytes());
            data.extend_from_slice(&0u32.to_be_bytes());
        }
        data.extend_from_slice(&WORD.to_be_bytes().repeat((NUM_RECORDS + WORD) as usize));
        let length = data.len() as u32;
        data[2..6].copy_from_slice(&length.to_be_bytes());
        // 1000 tables of 1284 bytes in 6 KB of data
        let selectors = IntSet::from_iter(0xe0100..0xe0100 + NUM_RECORDS);
        assert!(!uvs_ops_fit(&data, &selectors));
    }
}

#[test]
fn shared_uvs_tables_over_budget_fail_to_subset() {
    // a font whose cmap or DMAP has a format 14 subtable of `num_records` records
    // sharing their UVS tables, subset with all of the records
    let subset = |num_records: u32, in_dmap: bool| {
        let nominal = format12(&[(65, 1)]);
        let shared = shared_uvs_subtable(14, num_records);
        let cmap = table(&[(0, 4, &nominal), (0, 5, &shared)]);
        let plain = table(&[(0, 4, &nominal)]);
        let data = if in_dmap {
            font_with_cmaps(Some(&plain), Some(&cmap))
        } else {
            font_with_cmaps(Some(&cmap), None)
        };
        let font = FontRef::new(&data).unwrap();
        let mut unicodes = IntSet::from_iter([65]);
        unicodes.extend(0xe0100..0xe0100 + num_records);
        let plan = Plan::new(
            &IntSet::empty(),
            &unicodes,
            &font,
            SubsetFlags::SUBSET_FLAGS_DEFAULT,
            &IntSet::<Tag>::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::<NameId>::empty(),
            &IntSet::empty(),
        );
        subset_font(&font, &plan).map(|_| ())
    };
    assert!(subset(2, false).is_ok());
    assert!(matches!(subset(200, false), Err(SubsetTableError(tag)) if tag == Cmap::TAG));
    assert!(subset(2, true).is_ok());
    assert!(matches!(subset(200, true), Err(SubsetTableError(tag)) if tag == Dmap::TAG));
}

#[test]
fn cmap15_subset_planner_closes_preferred_variation_glyphs() {
    let nominal = format12(&[(65, 1)]);
    let narrow = format14(0xe0100, &[], &[(65, 24)]);
    let wide = format15(0xe0100, &[], &[(65, 25)]);
    let cmap = table(&[(0, 4, &nominal), (0, 5, &narrow), (0, 5, &wide)]);
    let data = font_with_cmaps(Some(&cmap), None);
    let font = FontRef::new(&data).unwrap();
    let plan = Plan::new(
        &IntSet::empty(),
        &IntSet::from_iter([65, 0xe0100]),
        &font,
        SubsetFlags::SUBSET_FLAGS_DEFAULT,
        &IntSet::<Tag>::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::<NameId>::empty(),
        &IntSet::empty(),
    );
    assert!(plan.unicodes.contains(0xe0100));
    assert!(plan.glyphset_gsub.contains(GlyphId::new(25)));
    assert!(!plan.glyphset_gsub.contains(GlyphId::new(24)));
    assert_eq!(plan.num_output_glyphs, 3);
    let output = subset_font(&font, &plan).unwrap();
    let output = FontRef::new(&output).unwrap();
    let cmap = output.cmap().unwrap();
    let (_, variation) = cmap.uvs_subtable().unwrap();
    assert_eq!(variation.format(), 15);
    assert_eq!(
        variation.map_variant(65u32, 0xe0100u32),
        Some(MapVariant::Variant(GlyphId::new(2)))
    );
    assert_eq!(output.maxp().unwrap().num_glyphs(), 3);
}

/// A default UVS range starting at U+0000 is kept, like harfbuzz does.
#[test]
fn cmap14_default_uvs_keeps_range_starting_at_zero() {
    let data = format14(0xe0100, &[0, 65, 66], &[]);
    let source = CmapSubtable::read(FontData::new(&data)).unwrap();
    let mut plan = Plan::default();
    plan.unicodes.extend([0, 65, 0xe0100]);
    let output = serialize_variant(&source, &plan).unwrap();
    let output = Cmap14::read(FontData::new(&output)).unwrap();
    let output = read_fonts::tables::cmap::UvsSubtable::from(output);
    for (cp, expected) in [
        (0, Some(MapVariant::UseDefault)),
        (65, Some(MapVariant::UseDefault)),
        (66, None),
    ] {
        assert_eq!(output.map_variant(cp as u32, 0xe0100u32), expected, "{cp}");
    }
}
