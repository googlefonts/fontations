//! splitting layout (GPOS) subtables

use std::collections::{HashMap, HashSet};

use font_types::{GlyphId, Offset16};

use super::{Graph, ObjectId};

use crate::types::FixedSize;
use crate::{tables::layout as wlayout, write::TableData};

use read_fonts::tables::{gpos as rgpos, layout as rlayout};

const MAX_TABLE_SIZE: usize = u16::MAX as usize;

pub(super) fn split_pair_pos(graph: &mut Graph, lookup: ObjectId) {
    split_subtables(graph, lookup, split_pair_pos_subtable)
}

/// A common impl handling updating the lookup with the new subtables
fn split_subtables(
    graph: &mut Graph,
    lookup: ObjectId,
    split_fn: fn(&mut Graph, ObjectId) -> Option<Vec<ObjectId>>,
) {
    let data = graph.objects.remove(&lookup).unwrap();
    debug_assert!(
        data.reparse::<rgpos::PositionLookup>().is_ok(),
        "table splitting is only relevant for GPOS?"
    );
    log::debug!("trying to split subtables in {lookup:?}, '{}'", data.type_);

    let mut new_subtables = HashMap::new();
    for subtable in &data.offsets {
        if let Some(split_subtables) = split_fn(graph, subtable.object) {
            new_subtables.insert(subtable.object, split_subtables);
        }
    }

    if new_subtables.is_empty() {
        // just put the old data back unchanged; nothing to see here
        graph.objects.insert(lookup, data);
        log::debug!("nothing to split, continuing");
        return;
    }

    let n_new_subtables = new_subtables
        .values()
        // - 1 because each group of new subtables replaces an old subtable
        .map(|ids| ids.len() - 1)
        .sum::<usize>();

    let n_total_subtables: u16 = (data.offsets.len() + n_new_subtables).try_into().unwrap();
    // we just want the lookup type/flag/etc, but we need a generic FontRead type
    let generic_lookup: rlayout::Lookup<()> = data.reparse().unwrap();
    let mut new_data = TableData::new(data.type_);
    new_data.write(generic_lookup.lookup_type());
    new_data.write(generic_lookup.lookup_flag());
    new_data.write(n_total_subtables);
    for sub in data.offsets {
        match new_subtables.get(&sub.object) {
            Some(new) => new.iter().for_each(|id| new_data.add_offset(*id, 2, 0)),
            None => new_data.add_offset(sub.object, 2, 0),
        }
    }

    graph.nodes.get_mut(&lookup).unwrap().size = new_data.bytes.len() as _;
    graph.objects.insert(lookup, new_data);
}

fn split_pair_pos_subtable(graph: &mut Graph, lookup: ObjectId) -> Option<Vec<ObjectId>> {
    let data = &graph.objects[&lookup];
    let format: u16 = data.read_at(0).unwrap();
    match format {
        1 => split_pair_pos_format_1(graph, lookup),
        2 => {
            log::info!("table splitting not yet implemented for PairPos format 2");
            None
        }
        other => {
            log::warn!("unexpected pairpos format '{other}'");
            None
        }
    }
}

// based off of
// <https://github.com/harfbuzz/harfbuzz/blob/5d543d64222c6ce45332d0c188790f90691ef112/src/graph/pairpos-graph.hh#L50>
fn split_pair_pos_format_1(graph: &mut Graph, subtable: ObjectId) -> Option<Vec<ObjectId>> {
    const BASE_SIZE: usize = 5 * u16::RAW_BYTE_LEN;

    let data = &graph.objects[&subtable];

    debug_assert!(data.reparse::<rgpos::PairPosFormat1>().is_ok());
    let coverage_id = data.offsets.first().unwrap();
    assert_eq!(coverage_id.pos, 2, "offset records are always sorted");
    let coverage_size = graph.objects[&coverage_id.object].bytes.len();

    let mut visited = HashSet::with_capacity(data.offsets.len());

    let mut partial_coverage_size = 4;
    let mut accumulated = BASE_SIZE;
    let mut split_points = Vec::new();

    for (i, pair_set) in data.offsets.iter().skip(1).enumerate() {
        let table_size = if visited.insert(pair_set.object) {
            graph.objects[&pair_set.object].bytes.len()
        } else {
            0
        };
        let accumulated_delta = table_size +
            // the offset to the table
            Offset16::RAW_BYTE_LEN;
        // another glyph in the coverage table
        partial_coverage_size += u16::RAW_BYTE_LEN;
        accumulated += accumulated_delta;
        let total = accumulated + coverage_size.min(partial_coverage_size);
        if total > MAX_TABLE_SIZE {
            log::debug!("adding split at {i}");
            split_points.push(i);
            accumulated = BASE_SIZE + accumulated_delta;
            partial_coverage_size = 6; // + one glyph, because this table didn't fit
            visited.clear();
        }
    }

    log::debug!(
        "nothing to split, size '{}'",
        accumulated + coverage_size.min(partial_coverage_size)
    );

    if split_points.is_empty() {
        return None;
    }
    split_points.push(data.offsets.len() - 1);
    // okay, now we have a list of split points.

    let mut new_subtables = Vec::new();
    let mut prev_idx = 0;
    for idx in split_points {
        // the split point is the *start* of the next subtable, so we do not
        // include this item in this subtable
        let end = idx as u16 - 1;
        let new_subtable = split_off_ppf1(graph, subtable, prev_idx, end);
        prev_idx = idx as u16;
        new_subtables.push(graph.add_object(new_subtable));
    }
    Some(new_subtables)
}

fn split_off_ppf1(graph: &mut Graph, subtable: ObjectId, start: u16, end: u16) -> TableData {
    let coverage = graph.objects[&subtable].offsets.first().unwrap().object;
    let coverage = graph.objects.get(&coverage).unwrap();
    let coverage = coverage.reparse::<rlayout::CoverageTable>().unwrap();
    let n_pair_sets = (end - start) + 1;
    let new_coverage = split_coverage(&coverage, start, end);
    let new_cov_id = graph.add_object(new_coverage);

    let data = &graph.objects[&subtable];
    let table = data.reparse::<rgpos::PairPosFormat1>().unwrap();

    let mut new_ppf1 = TableData::new(data.type_);

    new_ppf1.write(table.pos_format());
    new_ppf1.add_offset(new_cov_id, 2, 0);
    new_ppf1.write(table.value_format1());
    new_ppf1.write(table.value_format2());
    new_ppf1.write(n_pair_sets);
    for off in data.offsets[1 + start as usize..]
        .iter()
        .take(n_pair_sets as _)
    {
        new_ppf1.add_offset(off.object, 2, 0)
    }
    new_ppf1
}

fn split_coverage(coverage: &rlayout::CoverageTable, start: u16, end: u16) -> TableData {
    assert!(start <= end);
    let len = (end - start) + 1;
    let mut data = TableData::default();
    match coverage {
        rlayout::CoverageTable::Format1(table) => {
            data.write(1u16);
            data.write(len);
            for gid in &table.glyph_array()[start as usize..=end as usize] {
                data.write(gid.get());
            }
        }
        rlayout::CoverageTable::Format2(table) => {
            // we will stay in format2, but it's possible it is no longer best?
            let records = table
                .range_records()
                .iter()
                .filter_map(|record| split_range_record(record, start, end))
                .collect::<Vec<_>>();
            data.write(2u16);
            data.write(records.len() as u16);
            for record in records {
                data.write(record.start_glyph_id);
                data.write(record.end_glyph_id);
                data.write(record.start_coverage_index);
            }
        }
    }
    data
}

fn split_range_record(
    record: &rlayout::RangeRecord,
    start: u16,
    end: u16,
) -> Option<wlayout::RangeRecord> {
    // the range is a range of coverage indices, not of glyph ids!
    let cov_start = record.start_coverage_index();
    let len = record.end_glyph_id().to_u16() - record.start_glyph_id().to_u16();
    let cov_range = cov_start..cov_start + len;

    if cov_range.start > end || cov_range.end < start {
        return None;
    }

    // okay, so we intersect. what is our start_coverage_index?

    // the new start is the number of items in the subset range that occur
    // before the first item in this record
    let new_cov_start = cov_range.start.saturating_sub(start);

    let start_glyph_delta = start.saturating_sub(cov_range.start);
    // the start is the old start + the number of glyphs truncated from the record
    let start_glyph = record.start_glyph_id().to_u16() + start_glyph_delta;
    let range_len = cov_range.end.min(end) - cov_range.start.max(start);

    let end_glyph = start_glyph + range_len;
    Some(wlayout::RangeRecord::new(
        GlyphId::new(start_glyph),
        GlyphId::new(end_glyph),
        new_cov_start,
    ))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, ops::Range};

    use read_fonts::{tables::layout::LookupFlag, FontData, FontRead};

    use super::*;
    use crate::{
        tables::{
            gpos::{PairPos, PairSet, PairValueRecord, ValueRecord},
            layout::CoverageTableBuilder,
        },
        TableWriter,
    };

    fn make_read_record(start_coverage_index: u16, glyphs: Range<u16>) -> rlayout::RangeRecord {
        rlayout::RangeRecord {
            start_glyph_id: GlyphId::new(glyphs.start).into(),
            end_glyph_id: GlyphId::new(glyphs.end).into(),
            start_coverage_index: start_coverage_index.into(),
        }
    }

    #[test]
    fn splitting_range_records() {
        let record = make_read_record(10, 20..30);
        // fully before: no result
        assert!(split_range_record(&record, 0, 5).is_none());

        // just first item:
        let split = split_range_record(&record, 5, 10).unwrap();
        assert_eq!(split.start_glyph_id.to_u16(), 20);
        assert_eq!(split.end_glyph_id.to_u16(), 20);
        assert_eq!(split.start_coverage_index, 5);

        // overlapping at start
        let split = split_range_record(&record, 8, 12).unwrap();
        assert_eq!(split.start_glyph_id.to_u16(), 20);
        assert_eq!(split.end_glyph_id.to_u16(), 22);
        assert_eq!(split.start_coverage_index, 2);

        // range is interior
        let split = split_range_record(&record, 12, 15).unwrap();
        assert_eq!(split.start_glyph_id.to_u16(), 22);
        assert_eq!(split.end_glyph_id.to_u16(), 25);
        assert_eq!(split.start_coverage_index, 0);

        // overlapping at end
        let split = split_range_record(&record, 18, 32).unwrap();
        assert_eq!(split.start_glyph_id.to_u16(), 28);
        assert_eq!(split.end_glyph_id.to_u16(), 30);
        assert_eq!(split.start_coverage_index, 0);

        // fully covered
        let split = split_range_record(&record, 5, 32).unwrap();
        assert_eq!(split.start_glyph_id.to_u16(), 20);
        assert_eq!(split.end_glyph_id.to_u16(), 30);
        assert_eq!(split.start_coverage_index, 5);

        // identical
        let split = split_range_record(&record, 10, 20).unwrap();
        assert_eq!(split.start_glyph_id.to_u16(), 20);
        assert_eq!(split.end_glyph_id.to_u16(), 30);
        assert_eq!(split.start_coverage_index, 0);

        // fully after
        assert!(split_range_record(&record, 30, 35).is_none());
    }

    #[test]
    fn simple_split_at_end() {
        let record = make_read_record(0, 0..4);
        let split = split_range_record(&record, 2, 6).unwrap();
        assert_eq!(split.start_glyph_id.to_u16(), 2);
        assert_eq!(split.end_glyph_id.to_u16(), 4);
    }

    #[test]
    fn split_inclusive() {
        let record = make_read_record(0, 0..100);
        let head = split_range_record(&record, 0, 50).unwrap();
        assert_eq!(head.start_glyph_id.to_u16(), 0);
        assert_eq!(head.end_glyph_id.to_u16(), 50);
        let tail = split_range_record(&record, 50, 100).unwrap();
        assert_eq!(tail.start_glyph_id.to_u16(), 50);
        assert_eq!(tail.end_glyph_id.to_u16(), 100);
    }

    // a big empty smoke test that constructs a real table and splits it
    #[test]
    fn split_pair_pos1() {
        let _ = env_logger::builder().is_test(true).try_init();

        struct KernPair(GlyphId, GlyphId, i16);
        fn make_pair_pos(pairs: Vec<KernPair>) -> PairPos {
            let mut records = BTreeMap::new();
            for KernPair(one, two, kern) in pairs {
                let value_record1 = ValueRecord::new()
                    .with_x_advance(kern)
                    .with_x_placement(kern)
                    .with_y_advance(kern)
                    .with_y_placement(kern);
                records
                    .entry(one)
                    .or_insert_with(PairSet::default)
                    .pair_value_records
                    .push(PairValueRecord {
                        second_glyph: two,
                        value_record1,
                        value_record2: ValueRecord::default(),
                    })
            }

            let coverage: CoverageTableBuilder = records.keys().copied().collect();
            let pair_sets = records.into_values().collect();

            PairPos::format_1(coverage.build(), pair_sets)
        }

        const N_GLYPHS: u16 = 1500; // manually determined to cause overflow

        let mut pairs = Vec::new();
        for (advance, g1) in [0u16..N_GLYPHS].into_iter().flatten().enumerate() {
            pairs.push(KernPair(GlyphId::new(g1), GlyphId::new(5), advance as _));
            pairs.push(KernPair(GlyphId::new(g1), GlyphId::new(6), advance as _));
            pairs.push(KernPair(GlyphId::new(g1), GlyphId::new(7), advance as _));
            pairs.push(KernPair(GlyphId::new(g1), GlyphId::new(8), advance as _));
        }

        let table = make_pair_pos(pairs);
        let lookup = wlayout::Lookup::new(LookupFlag::empty(), vec![table], 0);
        let mut graph = TableWriter::make_graph(&lookup);

        let id = graph.root;
        split_pair_pos(&mut graph, id);
        graph.remove_orphans();
        assert!(graph.basic_sort());

        let bytes = graph.serialize();

        let lookup = rlayout::Lookup::<rgpos::PairPosFormat1>::read(FontData::new(&bytes)).unwrap();
        assert_eq!(lookup.sub_table_count(), 2);
        let sub1 = lookup.subtables().get(0).unwrap();
        let sub2 = lookup.subtables().get(1).unwrap();

        // ensure that the split coverage tables equal the unsplit coverage table
        let gids = sub1
            .coverage()
            .unwrap()
            .iter()
            .chain(sub2.coverage().unwrap().iter())
            .map(GlyphId::to_u16)
            .collect::<Vec<_>>();
        assert_eq!(gids.len(), N_GLYPHS as _);

        let expected = std::iter::successors(Some(0), |n| Some(n + 1))
            .take(N_GLYPHS as _)
            .collect::<Vec<_>>();
        assert_eq!(gids, expected);

        // ensure that the PairSet tables at the split boundaries are as expected
        assert_eq!(sub1.pair_set_count() + sub2.pair_set_count(), N_GLYPHS);
    }
}
