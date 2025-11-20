//! Split Coverage table in a graph

use crate::{
    graph::{Graph, RepackError},
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
) -> Result<Vec<GlyphId>, RepackError> {
    let coverage_data = graph
        .vertex_data(cov_idx)
        .ok_or(RepackError::GraphErrorInvalidObjIndex)?;

    let coverage_table = CoverageTable::read(FontData::new(coverage_data))
        .map_err(|_| RepackError::ErrorReadTable)?;

    Ok(coverage_table.iter().map(GlyphId::from).collect())
}

// Make a coverage table at the specified coverage vertex
pub(crate) fn make_coverage(
    graph: &mut Graph,
    coverage_idx: ObjIdx,
    cov_glyphs: &[GlyphId],
    glyph_range: Range<usize>,
) -> Result<(), RepackError> {
    let glyphs = cov_glyphs
        .get(glyph_range)
        .ok_or(RepackError::ErrorSplitSubtable)?;

    let mut s = Serializer::new(glyphs.len() * 6 + 4);
    s.start_serialize()
        .map_err(|_| RepackError::ErrorRepackSerialize)?;

    CoverageTable::serialize(&mut s, glyphs).map_err(|_| RepackError::ErrorRepackSerialize)?;
    s.end_serialize();

    let coverage_data = s.copy_bytes();
    graph.update_vertex_data(coverage_idx, &coverage_data)
}
