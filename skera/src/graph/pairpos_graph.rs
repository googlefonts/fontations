//! Split  PairPos table in a graph
use crate::{
    graph::{
        coverage_graph::{add_new_coverage, coverage_glyphs, make_coverage},
        layout::DataBytes,
        Graph, RepackError,
    },
    serialize::{Link, LinkWidth, ObjIdx, Serializer},
    Serialize,
};
use std::collections::BTreeMap;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gpos::ValueFormat,
            layout::{ClassDef, CoverageTable},
        },
        FontData, FontRead,
    },
    types::{FixedSize, GlyphId, Offset16, Scalar},
};

// output only contains new subtable indices
// ref:<https://github.com/harfbuzz/harfbuzz/blob/708bf4a0c80b9f323c9a1c8ec00ff9c2cb429b1f/src/graph/pairpos-graph.hh#L607>
pub(crate) fn split_pairpos(
    graph: &mut Graph,
    table_idx: ObjIdx,
) -> Result<Vec<ObjIdx>, RepackError> {
    let format_bytes = graph
        .vertex_data(table_idx)
        .ok_or(RepackError::GraphErrorInvalidObjIndex)?
        .get(0..2)
        .unwrap();
    let format = u16::read(format_bytes).ok_or(RepackError::ErrorReadTable)?;
    match format {
        1 => split_format1(graph, table_idx),
        2 => split_format2(graph, table_idx),
        _ => Err(RepackError::ErrorReadTable),
    }
}

fn split_format1(graph: &mut Graph, table_idx: ObjIdx) -> Result<Vec<ObjIdx>, RepackError> {
    let coverage_idx = graph
        .index_for_position(table_idx, PairPosFormat1::COVERAGE_OFFSET_POS)
        .ok_or(RepackError::ErrorReadTable)?;
    let all_glyphs = coverage_glyphs(graph, coverage_idx)?;
    let num_pair_sets = PairPosFormat1::from_graph(graph, table_idx)?.num_pair_sets() as usize;
    let split_points = compute_format1_split_points(graph, table_idx, coverage_idx, num_pair_sets)?;
    if split_points.is_empty() {
        return Ok(Vec::new());
    }

    let mut new_table_indices = Vec::new();
    for i in 0..split_points.len() {
        let start = split_points[i];
        let end = if i + 1 < split_points.len() {
            split_points[i + 1]
        } else {
            num_pair_sets
        };
        let new_idx = clone_range_format1(graph, table_idx, start, end, &all_glyphs)?;
        new_table_indices.push(new_idx);
    }

    shrink_format1(graph, table_idx, coverage_idx, &all_glyphs, split_points[0])?;

    Ok(new_table_indices)
}

fn clone_range_format1(
    graph: &mut Graph,
    table_idx: ObjIdx,
    start: usize,
    end: usize,
    coverage_glyphs: &[GlyphId],
) -> Result<ObjIdx, RepackError> {
    let new_pair_set_count = end - start;
    let new_table_size = PairPosFormat1::MIN_SIZE + new_pair_set_count * Offset16::RAW_BYTE_LEN;
    let new_table_idx = graph.new_vertex(new_table_size);

    let new_coverage_idx = graph.new_vertex(0);
    make_coverage(graph, new_coverage_idx, coverage_glyphs, start..end)?;

    graph.add_parent_child_link(
        new_table_idx,
        new_coverage_idx,
        LinkWidth::Two,
        PairPosFormat1::COVERAGE_OFFSET_POS,
        false,
    )?;

    // Copy value formats from original table
    let (v_fmt1, v_fmt2) = {
        let old_table = PairPosFormat1::from_graph(graph, table_idx)?;
        (old_table.value_format1(), old_table.value_format2())
    };

    let mut new_table = PairPosFormat1::from_graph(graph, new_table_idx)?;
    new_table.set_format(1);
    new_table.set_value_format1(v_fmt1);
    new_table.set_value_format2(v_fmt2);
    new_table.set_pair_set_count(new_pair_set_count as u16);

    // Move pair sets
    for i in 0..new_pair_set_count {
        let old_pos = PairPosFormat1::PAIR_SET_OFFSETS_START + (start + i) as u32 * 2;
        let new_pos = PairPosFormat1::PAIR_SET_OFFSETS_START + i as u32 * 2;
        graph.move_child(table_idx, old_pos, new_table_idx, new_pos, 2)?;
    }

    Ok(new_table_idx)
}

fn compute_format1_split_points(
    graph: &mut Graph,
    table_idx: ObjIdx,
    coverage_idx: ObjIdx,
    num_pair_sets: usize,
) -> Result<Vec<usize>, RepackError> {
    if num_pair_sets == 0 {
        return Ok(Vec::new());
    }

    let table_links = graph
        .vertex(table_idx)
        .ok_or(RepackError::GraphErrorInvalidObjIndex)?
        .real_links();
    let coverage_table_size = graph
        .vertex(coverage_idx)
        .ok_or(RepackError::GraphErrorInvalidObjIndex)?
        .table_size();

    let mut accumulated = PairPosFormat1::MIN_SIZE;
    let mut out = Vec::new();
    let mut visited = IntSet::empty();

    // Coverage table size estimate
    // For Format 1, it's 4 + 2 * num_glyphs
    // Since we are splitting by PairSet, each new subtable's coverage will have (end - start) glyphs
    let mut partial_coverage_size = 4;

    for i in 0..num_pair_sets {
        let pos =
            PairPosFormat1::PAIR_SET_OFFSETS_START + (i as u32 * Offset16::RAW_BYTE_LEN as u32);
        let Some(pairset_idx) = table_links.get(&pos).map(|l| l.obj_idx()) else {
            continue;
        };

        // Each PairSet adds an offset in the main table (2 bytes) and a glyph in the coverage table (2 bytes)
        partial_coverage_size += Offset16::RAW_BYTE_LEN;

        let pairset_size = graph.find_subgraph_size(pairset_idx, &mut visited, u16::MAX)?;
        accumulated += pairset_size + Offset16::RAW_BYTE_LEN;

        if accumulated + partial_coverage_size.min(coverage_table_size) > u16::MAX as usize {
            out.push(i);
            // Reset for the next subtable
            accumulated = PairPosFormat1::MIN_SIZE + Offset16::RAW_BYTE_LEN + pairset_size;
            partial_coverage_size = 4 + Offset16::RAW_BYTE_LEN;
            visited.clear();
        }
    }

    Ok(out)
}

fn shrink_format1(
    graph: &mut Graph,
    table_idx: ObjIdx,
    coverage_idx: ObjIdx,
    coverage_glyphs: &[GlyphId],
    shrink_point: usize,
) -> Result<(), RepackError> {
    {
        let mut table = PairPosFormat1::from_graph(graph, table_idx)?;
        let old_pair_set_count = table.num_pair_sets() as usize;
        if shrink_point >= old_pair_set_count {
            return Ok(());
        }
        table.set_pair_set_count(shrink_point as u16);
    }

    let table_v = graph
        .mut_vertex(table_idx)
        .ok_or(RepackError::GraphErrorInvalidObjIndex)?;
    table_v.tail = table_v.head + PairPosFormat1::MIN_SIZE + shrink_point * Offset16::RAW_BYTE_LEN;

    make_coverage(graph, coverage_idx, coverage_glyphs, 0..shrink_point)
}

struct Format2TableInfo {
    table_idx: ObjIdx,
    coverage_idx: ObjIdx,
    value_format1: u16,
    value_format2: u16,
    class_def1_idx: ObjIdx,
    class_def2_idx: ObjIdx,
    class1_count: u16,
    class2_count: u16,
    glyph_classes: Vec<(GlyphId, u16)>,
    total_value_len: u32,
    v1_len: u32,
    class1_record_size: usize,
    format1_device_indices: Vec<u8>,
    format2_device_indices: Vec<u8>,
}

fn split_format2(graph: &mut Graph, table_idx: ObjIdx) -> Result<Vec<ObjIdx>, RepackError> {
    Ok(Vec::new())
}

struct PairPosFormat1<'a>(DataBytes<'a>);
impl<'a> PairPosFormat1<'a> {
    const MIN_SIZE: usize = 10;
    const FORMAT_POS: usize = 0;
    const COVERAGE_OFFSET_POS: u32 = 2;
    const VALUE_FORMAT1_POS: usize = 4;
    const VALUE_FORMAT2_POS: usize = 6;
    const PAIR_SET_COUNT_POS: usize = 8;
    const PAIR_SET_OFFSETS_START: u32 = 10;

    fn from_graph(graph: &'a mut Graph, obj_idx: ObjIdx) -> Result<Self, RepackError> {
        let data_bytes = DataBytes::from_graph(graph, obj_idx)?;
        let pair_pos = Self(data_bytes);
        if !pair_pos.sanitize() {
            return Err(RepackError::ErrorReadTable);
        }
        Ok(pair_pos)
    }

    fn sanitize(&self) -> bool {
        self.0.len() >= Self::MIN_SIZE
    }

    fn set_format(&mut self, format: u16) {
        self.0.write_at(format, Self::FORMAT_POS);
    }

    fn value_format1(&self) -> u16 {
        self.0.read_at::<u16>(Self::VALUE_FORMAT1_POS)
    }

    fn set_value_format1(&mut self, format: u16) {
        self.0.write_at(format, Self::VALUE_FORMAT1_POS);
    }

    fn value_format2(&self) -> u16 {
        self.0.read_at::<u16>(Self::VALUE_FORMAT2_POS)
    }

    fn set_value_format2(&mut self, format: u16) {
        self.0.write_at(format, Self::VALUE_FORMAT2_POS);
    }

    fn num_pair_sets(&self) -> u16 {
        self.0.read_at::<u16>(Self::PAIR_SET_COUNT_POS)
    }

    fn set_pair_set_count(&mut self, count: u16) {
        self.0.write_at(count, Self::PAIR_SET_COUNT_POS);
    }
}
