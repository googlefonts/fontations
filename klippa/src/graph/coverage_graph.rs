//! Split Coverage table in a graph

use crate::{
    graph::{Graph, RepackErrorFlags},
    serialize::{ObjIdx, Serializer},
    Serialize,
};
use write_fonts::{
    read::{tables::layout::CoverageTable, FontData, FontRead},
    types::GlyphId,
};

use std::ops::Range;

pub(crate) fn coverage_glyphs(
    graph: &mut Graph,
    cov_idx: ObjIdx,
) -> Result<Vec<GlyphId>, RepackErrorFlags> {
    let coverage_data = graph
        .vertex_data(cov_idx)
        .ok_or(RepackErrorFlags::GraphErrorInvalidObjIndex)?;

    let coverage_table = CoverageTable::read(FontData::new(coverage_data))
        .map_err(|_| RepackErrorFlags::RepackErrorReadTable)?;

    Ok(coverage_table.iter().map(GlyphId::from).collect())
}

// Make a coverage table at the specified coverage vertex
pub(crate) fn make_coverage(
    graph: &mut Graph,
    glyphs: &[GlyphId],
    coverage_idx: ObjIdx,
    alloc_new_data: bool,
) -> Result<(), RepackErrorFlags> {
    let mut s = Serializer::new(glyphs.len() * 6 + 4);
    CoverageTable::serialize(&mut s, glyphs).map_err(|_| RepackErrorFlags::RepackErrorSerialize)?;

    let coverage_data = s.copy_bytes();
    graph.update_vertex_data(coverage_idx, &coverage_data, alloc_new_data)
}

pub(crate) fn filter_coverage(
    graph: &mut Graph,
    coverage_idx: ObjIdx,
    cov_glyphs: &[GlyphId],
    glyph_range: Range<usize>,
    alloc_new_data: bool,
) -> Result<(), RepackErrorFlags> {
    let glyphs = cov_glyphs
        .get(glyph_range)
        .ok_or(RepackErrorFlags::RepackErrorSplitSubtable)?;
    make_coverage(graph, glyphs, coverage_idx, alloc_new_data)
}
