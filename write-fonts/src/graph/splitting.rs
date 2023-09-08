//! splitting layout (GPOS) subtables

use std::collections::{BTreeSet, HashMap, HashSet};

use font_types::{FixedSize, GlyphId, Offset16};
use read_fonts::tables::{
    gpos::{self as rgpos, ValueFormat},
    layout as rlayout,
};

use super::{Graph, ObjectId};
use crate::{
    tables::layout as wlayout, write::OffsetRecord, write::TableData, FontWrite, TableWriter,
};

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
    log::debug!("trying to split subtables in '{}'", data.type_);

    let mut new_subtables = HashMap::new();
    for (i, subtable) in data.offsets.iter().enumerate() {
        if let Some(split_subtables) = split_fn(graph, subtable.object) {
            log::trace!("produced {} splits for subtable {i}", split_subtables.len());
            new_subtables.insert(subtable.object, split_subtables);
        }
    }

    if new_subtables.is_empty() {
        // just put the old data back unchanged; nothing to see here
        graph.objects.insert(lookup, data);
        log::debug!("Splitting produced no new subtables");
        return;
    }

    let n_new_subtables = new_subtables
        .values()
        // - 1 because each group of new subtables replaces an old subtable
        .map(|ids| ids.len() - 1)
        .sum::<usize>();
    log::debug!("Splitting produced {n_new_subtables} new subtables");

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
        2 => split_pair_pos_format_2(graph, lookup),
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

    for (i, pair_set_off) in data.offsets.iter().skip(1).enumerate() {
        let table_size = if visited.insert(pair_set_off.object) {
            let pairset = &graph.objects[&pair_set_off.object];
            // the size of the pairset table itself
            let mut subgraph_size = pairset.bytes.len();
            // now we add the lengths of any new device tables. we do *not*
            // deduplicate these, even within the graph, since it is possible
            // that they will need to be re-duplicated later in some cases?
            // TODO: investigate whether there are any wins here?
            // <https://github.com/googlefonts/fontations/issues/596>
            subgraph_size += pairset
                .offsets
                .iter()
                .map(|off| graph.objects[&off.object].bytes.len())
                .sum::<usize>();
            subgraph_size
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
            log::trace!("adding split at {i}");
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

// based off of
// <https://github.com/harfbuzz/harfbuzz/blob/f380a32825a1b2c51bbe21dc7acb9b3cc0921f69/src/graph/pairpos-graph.hh#L207>
fn split_pair_pos_format_2(graph: &mut Graph, subtable: ObjectId) -> Option<Vec<ObjectId>> {
    const BASE_SIZE: usize = 8 * u16::RAW_BYTE_LEN;
    let data = &graph.objects[&subtable];

    let pp2 = data.reparse::<rgpos::PairPosFormat2>().unwrap();
    let cur_len = data.bytes.len();
    log::info!(
        "PairPos f.2 subtable has {} class1 and {} class2, current size {cur_len} ",
        pp2.class1_count(),
        pp2.class2_count()
    );
    // we can't get these from the reparsed table because its offsets
    // are not valid until compiled into the final table
    let coverage_id = data.offsets[0].object;
    let class_def1_id = data.offsets[1].object;
    let class_def2_id = data.offsets[2].object;
    let class_def2_size = graph.objects[&class_def2_id].bytes.len();
    let coverage = &graph.objects[&coverage_id];
    let coverage = coverage.reparse::<rlayout::CoverageTable>().unwrap();

    let class_def1 = &graph.objects[&class_def1_id];
    let class_def1 = class_def1.reparse::<rlayout::ClassDef>().unwrap();
    let estimator = ClassDefSizeEstimator::new(coverage, class_def1);

    let class2_count = pp2.class2_count();
    let class1_record_size = class2_count as usize
        * (pp2.value_format1().record_byte_len() + pp2.value_format2().record_byte_len());

    let mut accumulated = BASE_SIZE;
    let mut coverage_size = 4;
    let mut class_def_1_size = 4;
    let mut max_coverage_size = coverage_size;
    let mut max_class_def_1_size = class_def_1_size;

    let mut split_points = Vec::new();
    let has_device_tables = (pp2.value_format1() | pp2.value_format2())
        .intersects(rgpos::ValueFormat::ANY_DEVICE_OR_VARIDX);

    let mut visited = HashSet::new();
    let mut next_device_offset = 3; // start after coverage + classs defs
    for (idx, class1rec) in pp2.class1_records().iter().enumerate() {
        let mut accumulated_delta = class1_record_size;
        coverage_size += estimator.increment_coverage_size(idx as _);
        class_def_1_size += estimator.increment_class_def_size(idx as _);
        max_coverage_size = max_coverage_size.max(coverage_size);
        max_class_def_1_size = max_class_def_1_size.max(class_def_1_size);

        // NOTE:
        //I'm finding that we generate slightly more splits than I would expect,
        //and I want to look into that, but i also want to get this merged.
        // Please remind me to open an issue to investigate size measurement
        // more thoroughly. In particular, look into why we just take subgraph
        // size for pairpos1 (ignoring duplicates) but try to account for duplicates
        // in pairpos2?
        // tracked at <https://github.com/googlefonts/fontations/issues/601>
        if has_device_tables {
            for class2rec in class1rec.unwrap().class2_records.iter() {
                let class2rec = class2rec.as_ref().unwrap();
                accumulated_delta += size_of_value_record_children(
                    &class2rec.value_record1,
                    graph,
                    &data.offsets,
                    &mut next_device_offset,
                    &mut visited,
                );
                accumulated_delta += size_of_value_record_children(
                    &class2rec.value_record2,
                    graph,
                    &data.offsets,
                    &mut next_device_offset,
                    &mut visited,
                );
            }
        }

        accumulated += accumulated_delta;
        let largest_obj = coverage_size.max(class_def_1_size).max(class_def2_size);
        let total = accumulated + coverage_size + class_def_1_size + class_def2_size
            // largest obj packs last and can overflow
            - largest_obj;

        if total > MAX_TABLE_SIZE {
            split_points.push(idx);
            // split does not include this class, so add it for the next iteration
            accumulated = BASE_SIZE + accumulated_delta;
            coverage_size = 4 + estimator.increment_coverage_size(idx as _);
            class_def_1_size = 4 + estimator.increment_class_def_size(idx as _);
            visited.clear();
        }
    }

    log::debug!("identified {} split points", split_points.len());
    if split_points.is_empty() {
        return None;
    }

    split_points.push(pp2.class1_count() as usize - 1);
    // now we have a list of split points, and just need to do the splitting.
    // note: harfbuzz does a thing here with a context type and an 'actuate_splits'
    // method.

    let mut new_subtables = Vec::new();
    let mut prev_subtable_start = 0;
    let mut next_device_offset = 3; // after coverage & two class defs
    for subtable_start in split_points {
        let subtable_end = subtable_start - 1;
        let (new_subtable, offsets_used) = split_off_ppf2(
            graph,
            subtable,
            prev_subtable_start,
            subtable_end,
            next_device_offset,
        );
        prev_subtable_start = subtable_start;
        next_device_offset += offsets_used;
        new_subtables.push(graph.add_object(new_subtable));
    }

    Some(new_subtables)
}

// returns the new table, + the number of non-null device offsets encountered.
fn split_off_ppf2(
    graph: &mut Graph,
    subtable: ObjectId,
    start: usize,
    end: usize,
    first_device_idx: usize,
) -> (TableData, usize) {
    // we have to do this bit manually (instead of via reparsing) because of borrowk
    let coverage = graph.objects[&subtable].offsets.first().unwrap().object;
    let coverage = graph.objects.get(&coverage).unwrap();
    let coverage = coverage.reparse::<rlayout::CoverageTable>().unwrap();
    let class_def_1 = graph.objects[&subtable].offsets[1].object;
    let class_def_1 = graph.objects.get(&class_def_1).unwrap();
    let class_def_1 = class_def_1.reparse::<rlayout::ClassDef>().unwrap();

    let n_records = (end - start) + 1;
    log::trace!("splitting off {n_records} class1records ({start}..={end})");

    let class_map = coverage
        .iter()
        .filter_map(|gid| {
            let glyph_class = class_def_1.get(gid);
            (start..=end)
                .contains(&(glyph_class as usize))
                // classes are used as indexes, so adjust them
                .then_some((gid, glyph_class.saturating_sub(start as u16)))
        })
        .collect::<HashMap<_, _>>();

    let new_coverage = class_map
        .keys()
        .copied()
        .collect::<wlayout::CoverageTable>();
    let new_coverage = make_table_data(&new_coverage);
    let new_cov_id = graph.add_object(new_coverage);
    let new_class_def1 = class_map
        .iter()
        .map(|tup| (*tup.0, *tup.1))
        .collect::<wlayout::ClassDef>();
    let new_class_def1 = make_table_data(&new_class_def1);
    let new_class_def1_id = graph.add_object(new_class_def1);
    // we reuse class2 without changing it. maybe we could be changing it?
    let class_def_2_id = graph.objects[&subtable].offsets[2].object;

    let data = &graph.objects[&subtable];
    let table = data.reparse::<rgpos::PairPosFormat2>().unwrap();
    let value_format1 = table.value_format1();
    let value_format2 = table.value_format2();

    let mut new_ppf2 = TableData::new(data.type_);
    new_ppf2.write(table.pos_format());
    new_ppf2.add_offset(new_cov_id, 2, 0);
    new_ppf2.write(value_format1);
    new_ppf2.write(value_format2);
    new_ppf2.add_offset(new_class_def1_id, 2, 0);
    new_ppf2.add_offset(class_def_2_id, 2, 0);

    // now we need to copy over the class1records
    let mut seen_offsets = 0;
    for class2rec in table
        .class1_records()
        .iter()
        .skip(start)
        .take(n_records)
        .flat_map(|c1rec| {
            c1rec
                .unwrap()
                .class2_records()
                .iter()
                .map(|rec| rec.unwrap())
        })
    {
        let rec_offset_start = first_device_idx + seen_offsets;
        let rec_offsets = &graph.objects[&subtable].offsets[rec_offset_start..];
        let rec1_seen = copy_value_rec(
            &mut new_ppf2,
            class2rec.value_record1(),
            value_format1,
            rec_offsets,
        );
        let rec_offsets = &rec_offsets[rec1_seen..];
        seen_offsets += rec1_seen;
        seen_offsets += copy_value_rec(
            &mut new_ppf2,
            class2rec.value_record2(),
            value_format2,
            rec_offsets,
        );
    }
    (new_ppf2, seen_offsets)
}

// returns the number of non-null offsets encountered in this record
fn copy_value_rec(
    target: &mut TableData,
    rec: &rgpos::ValueRecord,
    format: ValueFormat,
    dev_offsets: &[OffsetRecord],
) -> usize {
    let mut seen_offsets = 0;
    // a little macro to help us copy over all the fields.
    // - first we copy over the non-device tables
    // - then for the device tables, if they are present we copy over
    // the id.
    macro_rules! write_opt_field {
        ($fld:ident) => {
            if let Some(val) = rec.$fld() {
                target.write(val);
            }
        };
        ($fld:ident, $flag:expr) => {
            if !rec.$fld.get().is_null() {
                // we write this in a funny way to dodge a clippy warning
                seen_offsets += 1;
                target.add_offset(dev_offsets[seen_offsets - 1].object, 2, 0);
            } else if $flag {
                target.write(0u16); // null offset
            }
        };
    }
    write_opt_field!(x_placement);
    write_opt_field!(y_placement);
    write_opt_field!(x_advance);
    write_opt_field!(y_advance);

    write_opt_field!(
        x_placement_device,
        format.contains(ValueFormat::X_PLACEMENT_DEVICE)
    );
    write_opt_field!(
        y_placement_device,
        format.contains(ValueFormat::Y_PLACEMENT_DEVICE)
    );
    write_opt_field!(
        x_advance_device,
        format.contains(ValueFormat::X_ADVANCE_DEVICE)
    );
    write_opt_field!(
        y_advance_device,
        format.contains(ValueFormat::Y_ADVANCE_DEVICE)
    );
    seen_offsets
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

struct ClassDefSizeEstimator {
    consecutive_gids: bool,
    num_ranges_per_class: HashMap<u16, u16>,
    glyphs_per_class: HashMap<u16, BTreeSet<GlyphId>>,
}

const GLYPH_SIZE: usize = std::mem::size_of::<u16>();

impl ClassDefSizeEstimator {
    fn new(coverage: rlayout::CoverageTable, classdef: rlayout::ClassDef) -> Self {
        let mut consecutive_gids = true;
        let mut last_gid = None;
        let mut glyphs_per_class = HashMap::new();
        for (gid, class) in coverage.iter().map(|gid| (gid, classdef.get(gid))) {
            if let Some(last) = last_gid.take() {
                if last + 1 != gid.to_u16() {
                    consecutive_gids = false;
                }
            }
            last_gid = Some(gid.to_u16());
            glyphs_per_class
                .entry(class)
                .or_insert(BTreeSet::default())
                .insert(gid);
        }

        // now compute the number of ranges manually, skipping class 0
        let mut num_ranges_per_class = HashMap::with_capacity(glyphs_per_class.len());
        for (class, glyphs) in glyphs_per_class.iter().filter(|x| *x.0 != 0) {
            let num_ranges = count_num_ranges(glyphs);
            num_ranges_per_class.insert(*class, num_ranges);
        }
        ClassDefSizeEstimator {
            consecutive_gids,
            num_ranges_per_class,
            glyphs_per_class,
        }
    }

    fn n_glyphs_in_class(&self, class: u16) -> usize {
        self.glyphs_per_class
            .get(&class)
            .map(BTreeSet::len)
            .unwrap_or_default()
    }

    fn increment_coverage_size(&self, class: u16) -> usize {
        GLYPH_SIZE * self.n_glyphs_in_class(class)
    }

    fn increment_class_def_size(&self, class: u16) -> usize {
        // classdef2 uses 6 bytes for each range (start, end, class)
        const SIZE_PER_RANGE: usize = 6;
        let class_def_2_size = SIZE_PER_RANGE
            * self
                .num_ranges_per_class
                .get(&class)
                .copied()
                .unwrap_or_default() as usize;
        if self.consecutive_gids {
            class_def_2_size.min(self.n_glyphs_in_class(class) * GLYPH_SIZE)
        } else {
            class_def_2_size
        }
    }
}

fn count_num_ranges(glyphs: &BTreeSet<GlyphId>) -> u16 {
    let mut count = 0;
    let mut last = None;
    for gid in glyphs {
        match (last.take(), gid.to_u16()) {
            (Some(prev), current) if current == prev + 1 => (), // in same range
            _ => count += 1, // first glyph or glyph that starts new range
        }
        last = Some(gid.to_u16());
    }
    count
}

fn size_of_value_record_children(
    record: &rgpos::ValueRecord,
    graph: &Graph,
    offsets: &[OffsetRecord],
    // gets incremented every time we see a device offset
    next_offset_idx: &mut usize,
    seen: &mut HashSet<ObjectId>,
) -> usize {
    let subtables = [
        record.x_placement_device.get(),
        record.y_placement_device.get(),
        record.x_advance_device.get(),
        record.y_advance_device.get(),
    ];
    subtables
        .iter()
        .filter_map(|offset| (!offset.is_null()).then_some(*offset.offset()))
        .map(|_| {
            let obj = offsets[*next_offset_idx].object;
            if seen.insert(obj) {
                *next_offset_idx += 1;
                graph.objects[&obj].bytes.len()
            } else {
                0
            }
        })
        .sum()
}

// a helper to convert a write-fonts table into graph-ready bytes.
//
// NOTE: the table must not contain any offsets. intended for coverage/classdef
fn make_table_data(table: &dyn FontWrite) -> TableData {
    let mut writer = TableWriter::default();
    table.write_into(&mut writer);

    let mut r = writer.into_data();
    r.type_ = table.table_type();
    r
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, ops::Range};

    use read_fonts::{
        tables::{gpos::ValueFormat, layout::LookupFlag},
        FontData, FontRead,
    };

    use super::*;
    use crate::{
        tables::{
            gpos::{
                Class1Record, Class2Record, Gpos, PairPos, PairSet, PairValueRecord,
                PositionLookup, ValueRecord,
            },
            layout::{CoverageTableBuilder, DeviceOrVariationIndex, VariationIndex},
        },
        FontWrite, TableWriter,
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
        for (advance, g1) in (0u16..N_GLYPHS).enumerate() {
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

    fn make_pairpos_f1_with_device_tables(g1_count: u16, g2_count: u16) -> PairPos {
        let mut pairsets = Vec::new();
        for g1 in 1u16..=g1_count {
            let records = (1..=g2_count)
                .map(|gid2| {
                    let val = g2_count * g1 + gid2;
                    let valrec = ValueRecord::new()
                        .with_x_advance(val as _)
                        .with_y_advance(val as _)
                        .with_x_placement(val as _)
                        .with_y_advance_device(VariationIndex::new(val, val + 1))
                        .with_x_advance_device(VariationIndex::new(val, val));
                    let valrec2 = valrec
                        .clone()
                        .with_y_advance_device(VariationIndex::new(
                            u16::MAX - val,
                            u16::MAX - val - 1,
                        ))
                        .with_x_advance_device(VariationIndex::new(u16::MAX - val, u16::MAX - val));
                    PairValueRecord::new(GlyphId::new(gid2), valrec, valrec2)
                })
                .collect();
            pairsets.push(PairSet::new(records));
        }

        let coverage = (1u16..=g1_count).map(GlyphId::new).collect();
        PairPos::format_1(coverage, pairsets)
    }

    #[test]
    fn split_pairpos1_with_device_tables() {
        let _ = env_logger::builder().is_test(true).try_init();
        // construct a pp1 table that only requires splitting if you accounted
        // for device tables. so:

        // use device format with 5 fields, == 10 bytes per valuerecord,
        // 22 bytes per pairvaluerecord (gid + 2 * value record)
        // let's have two varidx tables per valuerecord, so...
        // 24 bytes of subtables (4 * 6)
        // let's say 100 pair value records per pairset, so:
        // pairset = (2 + 100 * 22) + (100 * 24) == 4602 bytes
        // so 15 pairsets (69030) puts us over the limit.
        const G1_COUNT: u16 = 15;
        const G2_COUNT: u16 = 100;

        // first just naively check that the split function, called directly,
        // works as expected

        let table = make_pairpos_f1_with_device_tables(G1_COUNT, G2_COUNT);
        let lookup = wlayout::Lookup::new(LookupFlag::empty(), vec![table], 0);
        let mut graph = TableWriter::make_graph(&lookup);

        assert!(lookup.table_type().is_splittable());
        let id = graph.root;
        split_pair_pos(&mut graph, id);
        graph.remove_orphans();
        assert!(graph.basic_sort());

        let bytes = graph.serialize();

        let rlookup =
            rlayout::Lookup::<rgpos::PairPosFormat1>::read(FontData::new(&bytes)).unwrap();
        assert_eq!(rlookup.sub_table_count(), 2);
    }

    #[test]
    fn fully_pack_pairpos1_with_device_tables() {
        // because we're good at packing, we need to include more tables in order
        // to trigger the splitting code, since if naive sorting succeeds we don't
        // bother. 28 PairSet tables is experimentally selected, requiring one split
        const G1_COUNT: u16 = 28;
        const G2_COUNT: u16 = 100;

        let table = make_pairpos_f1_with_device_tables(G1_COUNT, G2_COUNT);
        let lookup = wlayout::Lookup::new(LookupFlag::empty(), vec![table], 0);
        let lookuplist = wlayout::LookupList::new(vec![lookup]);
        assert!(crate::dump_table(&lookuplist).is_ok());
    }

    #[test]
    fn count_glyph_ranges() {
        fn make_input(glyphs: &[u16]) -> BTreeSet<GlyphId> {
            glyphs.iter().copied().map(GlyphId::new).collect()
        }

        assert_eq!(count_num_ranges(&make_input(&[])), 0);
        assert_eq!(count_num_ranges(&make_input(&[1])), 1);
        assert_eq!(count_num_ranges(&make_input(&[1, 2, 3])), 1);
        assert_eq!(count_num_ranges(&make_input(&[1, 2, 3])), 1);
        assert_eq!(count_num_ranges(&make_input(&[1, 2, 3, 5])), 2);
        assert_eq!(count_num_ranges(&make_input(&[1, 2, 3, 5, 6, 7, 10])), 3);
    }

    #[test]
    fn split_pair_pos2() {
        let _ = env_logger::builder().is_test(true).try_init();
        // okay so... I want a big pairpos format 2 table.
        // this means, mainly, that I want lots of different classes.

        fn next_class2_rec(i: usize) -> Class2Record {
            // idk how better to cast bits directly
            let val = i16::from_be_bytes(((i % u16::MAX as usize) as u16).to_be_bytes());
            // we add a device table every twelve records, arbitrary, we
            // want some but not a ton
            let value_format = ValueFormat::X_ADVANCE
                | ValueFormat::Y_ADVANCE
                | ValueFormat::X_PLACEMENT
                | ValueFormat::Y_PLACEMENT
                | ValueFormat::Y_ADVANCE_DEVICE;
            let add_device = i % 500 == 0;
            let mut record = ValueRecord::new()
                .with_explicit_value_format(value_format)
                .with_x_advance(val)
                .with_y_advance(val)
                .with_x_placement(val)
                .with_y_placement(val);
            if add_device {
                record = record.with_y_advance_device(DeviceOrVariationIndex::variation_index(
                    0xde, val as u16,
                ));
            }

            Class2Record {
                value_record1: record.clone(),
                value_record2: record,
            }
        }

        fn make_class_def(
            n_classes: u16,
            n_glyphs_per_class: u16,
            first_gid: u16,
        ) -> wlayout::ClassDef {
            let n_glyphs = n_classes * n_glyphs_per_class;
            (first_gid..first_gid + n_glyphs)
                .map(|gid| {
                    let class = (gid - 1) / n_glyphs_per_class;
                    (GlyphId::new(gid), class)
                })
                .collect()
        }
        const CLASS1_COUNT: u16 = 100;
        const CLASS2_COUNT: u16 = 100;

        let class_def1 = make_class_def(CLASS1_COUNT, 4, 1);
        let class_def2 = make_class_def(CLASS2_COUNT, 3, 1);

        assert_eq!(class_def1.class_count(), CLASS1_COUNT);
        assert_eq!(class_def2.class_count(), CLASS2_COUNT);
        let coverage = class_def1
            .iter()
            .map(|(gid, _)| gid)
            .collect::<wlayout::CoverageTable>();

        let class1recs = (0..CLASS1_COUNT)
            .map(|i| {
                Class1Record::new(
                    (0..CLASS2_COUNT)
                        .map(|j| next_class2_rec(i as usize * CLASS1_COUNT as usize + j as usize))
                        .collect(),
                )
            })
            .collect();

        let table = PairPos::format_2(coverage, class_def1, class_def2, class1recs);

        let lookup = wlayout::Lookup::new(LookupFlag::empty(), vec![table], 0);
        let lookup = PositionLookup::Pair(lookup);

        let lookup_list = wlayout::LookupList::new(vec![lookup]);
        let gpos = Gpos::new(Default::default(), Default::default(), lookup_list);
        let mut graph = TableWriter::make_graph(&gpos);

        graph.basic_sort();
        //graph.write_graph_viz("pairpos-test-0.dot").unwrap();
        assert!(graph.pack_objects());
        //graph.write_graph_viz("pairpos-test-1.dot").unwrap();
    }
}
