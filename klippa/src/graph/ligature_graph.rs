//! Split ligature substituation table in a graph

use crate::{
    graph::{
        coverage_graph::{coverage_glyphs, filter_coverage},
        layout::DataBytes,
        Graph, RepackErrorFlags,
    },
    serialize::{LinkWidth, ObjIdx},
};
use write_fonts::types::{FixedSize, GlyphId, Offset16};

pub(crate) fn split_ligature_subst(
    graph: &mut Graph,
    parent_idx: ObjIdx,
    table_idx: ObjIdx,
) -> Result<Vec<ObjIdx>, RepackErrorFlags> {
    let Some(coverage_idx) =
        graph.index_for_position(table_idx, LigatureSubstFormat1::COVERAGE_OFFSET_POS as u32)
    else {
        return Ok(Vec::new());
    };

    let lig_set_count = LigatureSubstFormat1::from_graph(graph, table_idx)?.lig_set_count();
    let split_points = compute_split_points(graph, table_idx, lig_set_count)?;
    if split_points.is_empty() {
        return Ok(Vec::new());
    }

    let cov_glyphs = coverage_glyphs(graph, coverage_idx)?;
    let this_index = graph
        .duplicate_child(parent_idx, table_idx)?
        .unwrap_or(table_idx);

    let mut out = Vec::with_capacity(split_points.len() + 1);
    for i in 0..split_points.len() {
        // [start,end) range
        let start = split_points[i];
        let end = if i < split_points.len() - 1 {
            split_points[i + 1]
        } else {
            (lig_set_count as u32) << 16
        };

        let new_idx = clone_range(
            graph,
            this_index,
            &cov_glyphs,
            lig_set_count as usize,
            start,
            end,
        )?;
        out.push(new_idx);
    }

    shrink(
        graph,
        this_index,
        coverage_idx,
        &cov_glyphs,
        split_points[0],
    )?;

    Ok(out)
}

fn compute_split_points(
    graph: &mut Graph,
    this_index: ObjIdx,
    lig_set_count: u16,
) -> Result<Vec<u32>, RepackErrorFlags> {
    let mut accumulated = LigatureSubstFormat1::MIN_SIZE;
    let mut out = Vec::new();
    for i in 0..lig_set_count as u32 {
        // offset to ligature set + LigatureSet table min_size
        accumulated += Offset16::RAW_BYTE_LEN + LigatureSet::MIN_SIZE;

        let pos = LigatureSubstFormat1::MIN_SIZE + i as usize * Offset16::RAW_BYTE_LEN;
        let Some(ligset_idx) = graph.index_for_position(this_index, pos as u32) else {
            return Ok(Vec::new());
        };

        let lig_set = LigatureSet::from_graph(graph, ligset_idx)?;
        let lig_count = lig_set.lig_count();
        for j in 0..lig_count as u32 {
            let Some(lig_idx) =
                graph.index_for_position(ligset_idx, 2 + j * Offset16::RAW_BYTE_LEN as u32)
            else {
                continue;
            };

            let lig_v = graph
                .vertex(lig_idx)
                .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?;

            let lig_size = lig_v.table_size();
            // offset to ligature + ligature table size
            accumulated += Offset16::RAW_BYTE_LEN + lig_size;
            if accumulated > u16::MAX as usize {
                out.push(i << 16 + j);

                // We're going to split such that the current ligature will be in the new sub table.
                // That means we'll have one ligature subst (base_base), one ligature set, and one liga table
                accumulated = LigatureSubstFormat1::MIN_SIZE
                    + Offset16::RAW_BYTE_LEN * 2
                    + LigatureSet::MIN_SIZE
                    + lig_size;
            }
        }
    }
    Ok(out)
}

fn clone_range(
    graph: &mut Graph,
    this_index: ObjIdx,
    cov_glyphs: &[GlyphId],
    lig_set_count: usize,
    start: u32,
    end: u32,
) -> Result<ObjIdx, RepackErrorFlags> {
    // Create an oversized new liga subst, we'll adjust the size down later. We don't know
    // the final size until we process it but we also need it to exist while we're processing
    // so that nodes can be moved to it as needed.
    let prime_size = LigatureSubstFormat1::MIN_SIZE + lig_set_count * Offset16::RAW_BYTE_LEN;

    let new_lig_subst_idx = graph.new_vertex(prime_size, true);
    //TODO:  lig_subst_prime.set_format()
    //TODO: table_idx: duplicate_if_shared

    // Create a place holder coverage prime id since we need to add virtual links to it while
    // generating liga and liga sets. Afterwards it will be updated to have the correct coverage.
    let new_coverage_idx = graph.new_vertex(0, false);
    graph.add_parent_child_link(
        new_lig_subst_idx,
        new_coverage_idx,
        LinkWidth::Two,
        2,
        false,
    )?;

    let start_lig_set_idx = start >> 16;
    let start_lig_idx = start & 0xFFFF;

    let end_lig_set_idx = end >> 16;
    let end_lig_idx = end & 0xFFFF;

    let mut new_lig_set_count = 0;
    for cur_lig_set_idx in start_lig_set_idx..=end_lig_set_idx {
        let ligset_pos = LigatureSubstFormat1::MIN_SIZE as u32 + cur_lig_set_idx * 2;
        let Some(ligset_graph_index) = graph.index_for_position(this_index, ligset_pos) else {
            continue;
        };

        let new_ligset_idx = if cur_lig_set_idx == start_lig_set_idx {
            if end_lig_set_idx != start_lig_set_idx {
                // move the entire ligature set to the new ligature table
                let lig_set_idx = graph.move_child(
                    this_index,
                    ligset_pos,
                    new_lig_subst_idx,
                    LigatureSubstFormat1::MIN_SIZE as u32 + new_lig_set_count * 2,
                    Offset16::RAW_BYTE_LEN,
                )?;
                compact_lig_set(graph, lig_set_idx)?;
                lig_set_idx
            } else {
                // This liga set partially overlaps [start, end). We'll need to create
                // a new liga set sub table and move the intersecting ligas to it.
                let num_liga = end_lig_idx - start_lig_idx;
                let new_ligset_table_idx = create_new_ligature_set(graph, num_liga as u16)?;
                graph.move_children(
                    ligset_graph_index,
                    LigatureSet::MIN_SIZE as u32 + start_lig_idx,
                    new_ligset_table_idx,
                    LigatureSet::MIN_SIZE as u32,
                    num_liga,
                    Offset16::RAW_BYTE_LEN,
                )?;
                // link new ligset
                graph.add_parent_child_link(
                    new_lig_subst_idx,
                    new_ligset_table_idx,
                    LinkWidth::Two,
                    LigatureSubstFormat1::MIN_SIZE as u32 + new_lig_set_count * 2,
                    false,
                )?;
                new_ligset_table_idx
            }
        } else if cur_lig_set_idx == end_lig_set_idx {
            if end_lig_idx == 0 {
                break;
            }
            // This liga set partially overlaps [start, end)
            let num_liga = end_lig_idx;
            let new_ligset_table_idx = create_new_ligature_set(graph, num_liga as u16)?;
            graph.move_children(
                ligset_graph_index,
                LigatureSet::MIN_SIZE as u32,
                new_ligset_table_idx,
                LigatureSet::MIN_SIZE as u32,
                num_liga,
                Offset16::RAW_BYTE_LEN,
            )?;

            // link new ligset
            graph.add_parent_child_link(
                new_lig_subst_idx,
                new_ligset_table_idx,
                LinkWidth::Two,
                LigatureSubstFormat1::MIN_SIZE as u32 + new_lig_set_count * 2,
                false,
            )?;

            new_ligset_table_idx
        } else {
            // This liga set is fully contained within [start, end)
            // We can move the entire ligaset to the new liga subset object.
            graph.move_child(
                this_index,
                ligset_pos,
                new_lig_subst_idx,
                LigatureSubstFormat1::MIN_SIZE as u32 + new_lig_set_count * 2,
                Offset16::RAW_BYTE_LEN,
            )?
        };
        new_lig_set_count += 1;

        // The new LigastureSet and all its children need to have a virtual link to the new coverage table
        fix_virtual_links(graph, new_ligset_idx, new_coverage_idx)?;
        let child_idxes: Vec<ObjIdx> = graph.vertices[new_ligset_idx]
            .real_links
            .values()
            .map(|l| l.obj_idx())
            .collect();
        for idx in child_idxes {
            fix_virtual_links(graph, idx, new_coverage_idx)?;
        }
    }

    graph.vertices[new_lig_subst_idx].tail -=
        (lig_set_count - new_lig_set_count as usize) * Offset16::RAW_BYTE_LEN;
    let mut new_lig_subst = LigatureSubstFormat1::from_graph(graph, new_lig_subst_idx)?;
    new_lig_subst.reset_lig_set_count(new_lig_set_count as u16);

    let end_glyph = if end_lig_idx == 0 {
        end_lig_set_idx
    } else {
        end_lig_set_idx + 1
    };

    filter_coverage(
        graph,
        new_coverage_idx,
        cov_glyphs,
        start_lig_set_idx as usize..end_glyph as usize,
        true,
    )?;
    Ok(new_lig_subst_idx)
}

// basically shrink the original LigatureSubst table by reducing the num of liga sets and coverage glyphs
// Note additional liga sets and ligatures should have been moved to the new LigatureSubst table by clone_range(),
// Note: We assume the input LigatureSubst table(subset output) already contains virtual links to Coverage table
// since we could reuse the existing coverage table obj_idx, so no need to clear and reset virtual links
fn shrink(
    graph: &mut Graph,
    table_idx: ObjIdx,
    coverage_idx: ObjIdx,
    cov_glyphs: &[GlyphId],
    shrink_point: u32,
) -> Result<(), RepackErrorFlags> {
    let end_lig_set_idx = shrink_point >> 16;
    let end_lig_idx = shrink_point & 0xFFFF;
    // adjust the num of ligatures in the last liga set if needed
    if end_lig_idx != 0 {
        let lig_set_idx = graph
            .index_for_position(
                table_idx,
                LigatureSubstFormat1::MIN_SIZE as u32 + end_lig_set_idx * 2,
            )
            .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?;

        let lig_set_v = graph
            .vertices
            .get_mut(lig_set_idx)
            .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?;
        let num_liga = lig_set_v.real_links.len();

        // sanity check: all additional links should have been moved to the new table
        // the num of real links left should equal to shrink point index + 1
        if num_liga != end_lig_set_idx as usize + 1 {
            return Err(RepackErrorFlags::RepackErrorSplitSubtable);
        }

        lig_set_v.tail = lig_set_v.head + LigatureSet::MIN_SIZE + num_liga * Offset16::RAW_BYTE_LEN;
        LigatureSet::from_graph(graph, lig_set_idx)?.reset_lig_count(num_liga as u16);
    }

    // adjust the num of liga sets in LigatureSubst table
    let table_v = graph
        .vertices
        .get_mut(table_idx)
        .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?;
    let num_remaining_liga_set = table_v.real_links.len();

    let num_liga_set = if end_lig_idx == 0 {
        end_lig_set_idx
    } else {
        end_lig_set_idx + 1
    };

    // sanity check
    if num_liga_set as usize != num_remaining_liga_set {
        return Err(RepackErrorFlags::RepackErrorSplitSubtable);
    }

    table_v.tail = table_v.head
        + LigatureSubstFormat1::MIN_SIZE
        + Offset16::RAW_BYTE_LEN * num_remaining_liga_set;
    LigatureSubstFormat1::from_graph(graph, table_idx)?.reset_lig_set_count(num_liga_set as u16);

    filter_coverage(
        graph,
        coverage_idx,
        cov_glyphs,
        0..num_remaining_liga_set,
        false,
    )
}

fn create_new_ligature_set(graph: &mut Graph, num_liga: u16) -> Result<ObjIdx, RepackErrorFlags> {
    let table_size = LigatureSet::MIN_SIZE + num_liga as usize * 2;
    let table_idx = graph.new_vertex(table_size, true);

    let table_head = graph
        .vertex(table_idx)
        .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?
        .head;

    graph
        .data
        .get_mut(table_head..table_head + 2)
        .ok_or(RepackErrorFlags::RepackErrorReadTable)?
        .copy_from_slice(&num_liga.to_be_bytes());

    Ok(table_idx)
}

fn compact_lig_set(graph: &mut Graph, liga_set_index: ObjIdx) -> Result<(), RepackErrorFlags> {
    let num_liga = graph
        .vertex(liga_set_index)
        .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?
        .real_links
        .len();

    let mut lig_set = LigatureSet::from_graph(graph, liga_set_index)?;
    let old_lig_count = lig_set.lig_count() as usize;
    if old_lig_count <= num_liga {
        return Ok(());
    }
    lig_set.reset_lig_count(num_liga as u16);

    let subtract_len = (old_lig_count - num_liga) * Offset16::RAW_BYTE_LEN;

    let lig_set_v = &mut graph.vertices[liga_set_index];
    let start_pos = LigatureSet::MIN_SIZE as u32;
    let mut new_links = Vec::with_capacity(num_liga as usize);
    for i in 0..old_lig_count as u32 {
        let old_pos = start_pos + i * 2;
        let Some((mut pos, mut l)) = lig_set_v.real_links.remove_entry(&old_pos) else {
            continue;
        };

        pos -= subtract_len as u32;
        l.update_position(pos);
        new_links.push((pos, l));
    }

    lig_set_v.real_links.extend(new_links);
    lig_set_v.tail -= subtract_len;
    Ok(())
}

// To make sure coverage table always packed at last(after LigatureSet and Ligature tables),
// add virtual links from the new liga set and all children to the new coverage table
// clear all existing virtual links first
fn fix_virtual_links(
    graph: &mut Graph,
    idx: ObjIdx,
    coverage_idx: ObjIdx,
) -> Result<(), RepackErrorFlags> {
    let child_idxes: Vec<ObjIdx> = graph.vertices[idx]
        .virtual_links
        .iter()
        .map(|l| l.obj_idx())
        .collect();

    for idx in child_idxes {
        graph
            .vertices
            .get_mut(idx)
            .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?
            .remove_parent(idx);
    }
    graph.vertices[idx].virtual_links.clear();
    graph.add_parent_child_link(idx, coverage_idx, LinkWidth::default(), 0, true)?;
    Ok(())
}

struct LigatureSubstFormat1<'a>(DataBytes<'a>);

impl<'a> LigatureSubstFormat1<'a> {
    const MIN_SIZE: usize = 6;
    const COVERAGE_OFFSET_POS: usize = 2;
    const LIG_SET_COUNT_POS: usize = 4;

    pub(crate) fn from_graph(
        graph: &'a mut Graph,
        obj_idx: ObjIdx,
    ) -> Result<Self, RepackErrorFlags> {
        let data_bytes = DataBytes::from_graph(graph, obj_idx)?;
        let lig_subst = Self(data_bytes);

        if !lig_subst.sanitize() {
            return Err(RepackErrorFlags::RepackErrorReadTable);
        }
        Ok(lig_subst)
    }

    fn sanitize(&self) -> bool {
        let len = self.0.len();
        if len < Self::MIN_SIZE {
            return false;
        }

        let lig_set_count = self.lig_set_count();
        len >= Self::MIN_SIZE + (lig_set_count as usize) * Offset16::RAW_BYTE_LEN
    }

    fn lig_set_count(&self) -> u16 {
        self.0.read_at::<u16>(Self::LIG_SET_COUNT_POS)
    }

    fn reset_lig_set_count(&mut self, lig_set_count: u16) {
        self.0.write_at(lig_set_count, Self::LIG_SET_COUNT_POS);
    }
}

struct LigatureSet<'a>(DataBytes<'a>);
impl<'a> LigatureSet<'a> {
    const MIN_SIZE: usize = 2;
    const LIGATURE_COUNT_POS: usize = 0;

    fn from_graph(graph: &'a mut Graph, obj_idx: ObjIdx) -> Result<Self, RepackErrorFlags> {
        let data_bytes = DataBytes::from_graph(graph, obj_idx)?;
        let lig_set = Self(data_bytes);

        if !lig_set.sanitize() {
            return Err(RepackErrorFlags::RepackErrorReadTable);
        }
        Ok(lig_set)
    }

    fn sanitize(&self) -> bool {
        let len = self.0.len();
        if len < Self::MIN_SIZE {
            return false;
        }

        let lig_count = self.lig_count() as usize;
        len >= Self::MIN_SIZE + lig_count * Offset16::RAW_BYTE_LEN
    }

    fn lig_count(&self) -> u16 {
        self.0.read_at::<u16>(Self::LIGATURE_COUNT_POS)
    }

    fn reset_lig_count(&mut self, lig_count: u16) {
        self.0.write_at(lig_count, Self::LIGATURE_COUNT_POS);
    }
}
