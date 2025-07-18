//! impl subset() for MarkRecord subtable
use crate::{CollectVariationIndices, Plan};
use write_fonts::read::{collections::IntSet, tables::gpos::MarkRecord, FontData};

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
