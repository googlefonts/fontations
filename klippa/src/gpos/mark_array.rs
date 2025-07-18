//! impl subset() for MarkRecord subtable
use crate::{CollectVariationIndices, Plan};
use fnv::FnvHashMap;
use write_fonts::read::{
    collections::IntSet,
    tables::{
        gpos::{MarkArray, MarkRecord},
        layout::{CoverageFormat1, CoverageFormat2, CoverageTable},
    },
    types::GlyphId,
    FontData,
};

pub(crate) fn collect_mark_record_varidx(
    mark_record: &MarkRecord,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
    font_data: FontData,
) {
    if let Ok(mark_anchor) = mark_record.mark_anchor(font_data) {
        mark_anchor.collect_variation_indices(plan, varidx_set);
    };
}

pub(crate) fn get_mark_class_map(
    covergae: &CoverageTable,
    mark_array: &MarkArray,
    glyph_set: &IntSet<GlyphId>,
) -> FnvHashMap<u16, u16> {
    let mark_records = mark_array.mark_records();

    let bsearch_length = match covergae {
        CoverageTable::Format1(t) => t.glyph_count(),
        CoverageTable::Format2(t) => t.range_count(),
    };
    let num_bits = 32 - bsearch_length.leading_zeros();
    let retained_classes: IntSet<u16> =
        if bsearch_length as u32 > (glyph_set.len() as u32) * num_bits {
            glyph_set
                .iter()
                .filter_map(|g| covergae.get(g))
                .filter_map(|idx| {
                    mark_records
                        .get(idx as usize)
                        .map(|mark_record| mark_record.mark_class())
                })
                .collect()
        } else {
            covergae
                .iter()
                .enumerate()
                .filter(|&(i, g)| glyph_set.contains(GlyphId::from(g)))
                .filter_map(|(idx, _)| {
                    mark_records
                        .get(idx)
                        .map(|mark_record| mark_record.mark_class())
                })
                .collect()
        };
    retained_classes
        .iter()
        .enumerate()
        .map(|(new_class, class)| (class, new_class as u16))
        .collect()
}
