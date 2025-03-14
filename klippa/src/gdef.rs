//! impl subset() for GDEF

use crate::{CollectVaritionaIndices, Plan};
use write_fonts::read::{
    collections::IntSet,
    tables::gdef::{CaretValue, CaretValueFormat3, Gdef, LigCaretList, LigGlyph},
    types::GlyphId,
};

impl CollectVaritionaIndices for Gdef<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Some(Ok(lig_caret_list)) = self.lig_caret_list() {
            lig_caret_list.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVaritionaIndices for LigCaretList<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Ok(coverage) = self.coverage() else {
            return;
        };

        let lig_glyphs = self.lig_glyphs();
        for (gid, lig_glyph) in coverage.iter().zip(lig_glyphs.iter()) {
            if lig_glyph.is_err() || !plan.glyphset_gsub.contains(GlyphId::from(gid)) {
                return;
            }
            lig_glyph
                .unwrap()
                .collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVaritionaIndices for LigGlyph<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        for caret in self.caret_values().iter() {
            if caret.is_err() {
                return;
            }
            caret.unwrap().collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVaritionaIndices for CaretValue<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        match self {
            Self::Format3(item) => item.collect_variation_indices(plan, varidx_set),
            _ => (),
        }
    }
}

impl CollectVaritionaIndices for CaretValueFormat3<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Ok(device) = self.device() {
            device.collect_variation_indices(plan, varidx_set);
        }
    }
}
