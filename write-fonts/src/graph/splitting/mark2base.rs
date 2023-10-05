//! splitting of MarkToBase positioning subtables

use std::collections::HashSet;

use font_types::{FixedSize, Offset16};
use read_fonts::tables::{gpos as rgpos, layout as rlayout};

use crate::{tables::layout::CoverageTable, write::TableData};

use super::{Graph, ObjectId};

pub(crate) fn split_mark_to_base(graph: &mut Graph, lookup: ObjectId) {
    super::split_subtables(graph, lookup, split_mark_to_base_subtable)
}

// based off of <https://github.com/harfbuzz/harfbuzz/blob/f26fd69d858642d764/src/graph/markbasepos-graph.hh#L212>
fn split_mark_to_base_subtable(graph: &mut Graph, subtable: ObjectId) -> Option<Vec<ObjectId>> {
    // the base size for the table, not the size of the 'base glyphs subtable'
    const BASE_SIZE: usize = 6 * u16::RAW_BYTE_LEN // six short fields in subtable
                               + u16::RAW_BYTE_LEN // empty mark array table
                               + u16::RAW_BYTE_LEN; // empty base array table
    let data = &graph.objects[&subtable];
    let base_coverage_id = data.offsets[1].object;
    let base_coverage_size = graph.objects[&base_coverage_id].bytes.len();
    debug_assert!(data.reparse::<rgpos::MarkBasePosFormat1>().is_ok());

    let min_subtable_size = BASE_SIZE + base_coverage_size;
    let class_info = get_class_info(graph, subtable);

    let base_array_off = &data.offsets[3];
    let base_array_data = &graph.objects[&base_array_off.object];

    let mark_class_count: u16 = data.read_at(6).unwrap_or(0);
    debug_assert_eq!(class_info.len(), mark_class_count as usize);
    let base_count: u16 = base_array_data.read_at(0).unwrap_or(0);

    let mut partial_coverage_size = 4;
    let mut accumulated = min_subtable_size;
    let mut split_points = Vec::new();
    let mut visited = HashSet::new();

    for i in 0..mark_class_count {
        let info = &class_info[i as usize];
        partial_coverage_size += u16::RAW_BYTE_LEN * info.marks.len();

        let mut accumulated_delta =
            // the records for the marks in this class
            rgpos::MarkRecord::RAW_BYTE_LEN * info.marks.len()
            // plus an offset in each base record for this class
            + Offset16::RAW_BYTE_LEN * base_count as usize;
        accumulated_delta += compute_subgraph_size(&info.children, graph, &mut visited);
        accumulated += accumulated_delta;
        let total = accumulated + partial_coverage_size;

        if total > super::MAX_TABLE_SIZE {
            log::trace!("adding split at {i}");
            split_points.push(i as usize);
            accumulated = min_subtable_size + accumulated_delta;
            partial_coverage_size = 4 + u16::RAW_BYTE_LEN * info.marks.len();
            visited.clear();
        }
    }

    log::debug!(
        "nothing to split, size '{}'",
        accumulated + partial_coverage_size
    );

    if split_points.is_empty() {
        return None;
    }

    split_points.push(mark_class_count as _);
    let mut new_subtables = Vec::new();
    let mut prev_split = 0;

    for next_split in split_points {
        let new_subtable = split_off_mark_pos(graph, subtable, prev_split, next_split, &class_info);
        prev_split = next_split;
        new_subtables.push(graph.add_object(new_subtable));
    }

    Some(new_subtables)
}

// based off of <https://github.com/harfbuzz/harfbuzz/blob/f26fd69d858642/src/graph/markbasepos-graph.hh#L411>
fn split_off_mark_pos(
    graph: &mut Graph,
    subtable: ObjectId,
    start: usize,
    end: usize,
    class_info: &[Mark2BaseClassInfo],
) -> TableData {
    let mark_coverage_id = graph.objects[&subtable].offsets.first().unwrap().object;
    let mark_coverage = &graph.objects[&mark_coverage_id];
    let mark_coverage = mark_coverage.reparse::<rlayout::CoverageTable>().unwrap();
    let data = &graph.objects[&subtable];
    let base_coverage_id = data.offsets[1].object;
    let mark_array_id = data.offsets[2].object;
    let base_array_id = data.offsets[3].object;
    let mark_class_count = (end - start) as u16;
    let mut new_subtable = TableData::new(data.type_);

    let mark_glyphs_by_cov_id: HashSet<_> = (start..end)
        .flat_map(|class_idx| class_info[class_idx].marks.iter())
        .copied()
        .collect();
    let new_mark_coverage: CoverageTable = mark_coverage
        .iter()
        .enumerate()
        .filter_map(|(i, gid)| mark_glyphs_by_cov_id.contains(&i).then_some(gid))
        .collect();
    let new_mark_coverage = super::make_table_data(&new_mark_coverage);
    let new_mark_coverage_id = graph.add_object(new_mark_coverage);
    let new_mark_array = split_off_mark_array(graph, mark_array_id, start, &mark_glyphs_by_cov_id);
    let new_mark_array_id = graph.add_object(new_mark_array);
    let new_base_array = split_off_base_array(graph, base_array_id, start, end, class_info.len());
    let new_base_array_id = graph.add_object(new_base_array);

    new_subtable.write(1u16); // format
    new_subtable.add_offset(new_mark_coverage_id, 2, 0);
    new_subtable.add_offset(base_coverage_id, 2, 0);
    new_subtable.write(mark_class_count);
    new_subtable.add_offset(new_mark_array_id, 2, 0);
    new_subtable.add_offset(new_base_array_id, 2, 0);

    new_subtable
}

// <https://github.com/harfbuzz/harfbuzz/blob/f26fd69d858642d76413b8f/src/graph/markbasepos-graph.hh#L170>
fn split_off_mark_array(
    graph: &Graph,
    mark_array: ObjectId,
    first_class: usize,
    mark_glyph_coverage_ids: &HashSet<usize>,
) -> TableData {
    let data = &graph.objects[&mark_array];
    let mark_array = data.reparse::<rgpos::MarkArray>().unwrap();
    let mark_count = mark_glyph_coverage_ids.len() as u16;

    let mut new_mark_array = TableData::new(data.type_);
    new_mark_array.write(mark_count);

    for (i, mark_record) in mark_array.mark_records().iter().enumerate() {
        if !mark_glyph_coverage_ids.contains(&i) {
            continue;
        }
        let new_class = mark_record.mark_class() - first_class as u16;
        let anchor_offset = data.offsets[i].object;

        new_mark_array.write(new_class);
        new_mark_array.add_offset(anchor_offset, 2, 0);
    }

    new_mark_array
}

fn split_off_base_array(
    graph: &Graph,
    base_array: ObjectId,
    start: usize,
    end: usize,
    old_mark_class_count: usize,
) -> TableData {
    let data = &graph.objects[&base_array];
    let mut new_base_array = TableData::new(data.type_);
    let base_count: u16 = data.read_at(0).unwrap_or(0);
    new_base_array.write(base_count);

    // the base array contains a (base_count x mark_count) matrix of offsets.
    // for each base, we want to prune the marks to only include those
    // in the range (start..end).

    debug_assert_eq!(
        data.offsets.len(),
        old_mark_class_count * base_count as usize
    );

    for base_record_offsets in data.offsets.chunks_exact(old_mark_class_count) {
        for offset_to_keep in &base_record_offsets[start..end] {
            new_base_array.add_offset(offset_to_keep.object, 2, 0)
        }
    }

    debug_assert_eq!(
        new_base_array.offsets.len(),
        (end - start) + base_count as usize
    );

    new_base_array
}

/// Information about a single mark class in a Mark2Base subtable
#[derive(Clone, Debug, Default)]
struct Mark2BaseClassInfo {
    // value is the order in the coverage table
    marks: HashSet<usize>,
    children: Vec<ObjectId>,
}

// this is not general purpose! tailored to mark2pos
fn compute_subgraph_size(
    objects: &[ObjectId],
    graph: &Graph,
    visited: &mut HashSet<ObjectId>,
) -> usize {
    objects
        .iter()
        .map(|id| {
            if !visited.insert(*id) {
                return 0;
            }
            // the size of the anchor table
            let base_size = graph.objects[id].bytes.len();
            // the size of any devices or variation indices.
            let children_size = graph.objects[id]
                .offsets
                .iter()
                .map(|id| {
                    // the mark2pos subgraph is only ever two layers deep
                    debug_assert!(graph.objects[&id.object].offsets.is_empty());
                    visited
                        .insert(id.object)
                        .then(|| graph.objects[&id.object].bytes.len())
                        .unwrap_or(0)
                })
                .sum::<usize>();
            base_size + children_size
        })
        .sum()
}

// get info about the mark classes in a MarkToBase subtable.
//
// based on <https://github.com/harfbuzz/harfbuzz/blob/f26fd69d858642d76/src/graph/markbasepos-graph.hh#L316>
fn get_class_info(graph: &Graph, subtable: ObjectId) -> Vec<Mark2BaseClassInfo> {
    let data = &graph.objects[&subtable];
    let mark_class_count: u16 = data.read_at(6).unwrap_or(0);
    let mut class_to_info = vec![Mark2BaseClassInfo::default(); mark_class_count as usize];
    let mark_array_off = &data.offsets[2];
    assert_eq!(mark_array_off.pos, 8);
    let mark_array_data = &graph.objects[&mark_array_off.object];
    let mark_array = mark_array_data.reparse::<rgpos::MarkArray>().unwrap();
    // okay so:
    // - there is one mark record for each mark glyph
    // - there may be multiple mark glyphs with the same class
    for (i, mark_record) in mark_array.mark_records().iter().enumerate() {
        let mark_class = mark_record.mark_class();
        // this shouldn't happen unless data is malformed? but harfbuzz includes
        // this check, and it doesn't hurt.
        if mark_class >= mark_class_count {
            continue;
        }
        let anchor_table_id = mark_array_data.offsets[i].object;
        class_to_info[mark_class as usize].marks.insert(i);
        class_to_info[mark_class as usize]
            .children
            .push(anchor_table_id);
    }

    // - base array declares one record for each base glyph (in cov table order)
    // - each record has an anchor for each mark glyph
    let base_array_off = &data.offsets[3];
    assert_eq!(base_array_off.pos, 10);
    let base_array_data = &graph.objects[&base_array_off.object];

    let base_count: u16 = base_array_data.read_at(0).unwrap_or(0);
    assert_eq!(
        base_array_data.offsets.len(),
        (base_count as usize * mark_class_count as usize)
    );

    for offsets in base_array_data.offsets.chunks_exact(mark_class_count as _) {
        for (i, off) in offsets.iter().enumerate() {
            class_to_info[i].children.push(off.object)
        }
    }

    class_to_info
}
