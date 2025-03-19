//! impl subset() for GDEF

use crate::{CollectVariationIndices, Plan};
use write_fonts::read::{
    collections::IntSet,
    tables::gdef::{CaretValue, Gdef, LigCaretList, LigGlyph, MarkGlyphSets},
    types::GlyphId,
};

impl CollectVariationIndices for Gdef<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Some(Ok(lig_caret_list)) = self.lig_caret_list() {
            lig_caret_list.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for LigCaretList<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Ok(coverage) = self.coverage() else {
            return;
        };

        let lig_glyphs = self.lig_glyphs();
        for (gid, lig_glyph) in coverage.iter().zip(lig_glyphs.iter()) {
            let Ok(lig_glyph) = lig_glyph else {
                return;
            };
            if !plan.glyphset_gsub.contains(GlyphId::from(gid)) {
                continue;
            }
            lig_glyph.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for LigGlyph<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        for caret in self.caret_values().iter() {
            let Ok(caret) = caret else {
                return;
            };
            caret.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for CaretValue<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Self::Format3(item) = self {
            if let Ok(device) = item.device() {
                device.collect_variation_indices(plan, varidx_set);
            }
        }
    }
}

pub(crate) trait CollectUsedMarkSets {
    fn collect_used_mark_sets(&self, plan: &Plan, used_mark_sets: &mut IntSet<u16>);
}

impl CollectUsedMarkSets for Gdef<'_> {
    fn collect_used_mark_sets(&self, plan: &Plan, used_mark_sets: &mut IntSet<u16>) {
        if let Some(Ok(mark_glyph_sets)) = self.mark_glyph_sets_def() {
            mark_glyph_sets.collect_used_mark_sets(plan, used_mark_sets);
        };
    }
}

impl CollectUsedMarkSets for MarkGlyphSets<'_> {
    fn collect_used_mark_sets(&self, plan: &Plan, used_mark_sets: &mut IntSet<u16>) {
        for (i, coverage) in self.coverages().iter().enumerate() {
            let Ok(coverage) = coverage else {
                return;
            };
            if coverage.intersects(&plan.glyphset_gsub) {
                used_mark_sets.insert(i as u16);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::{
        read::{FontRef, TableProvider},
        types::GlyphId,
    };
    #[test]
    fn test_collect_var_indices() {
        let mut plan = Plan::default();
        plan.glyphset_gsub.insert(GlyphId::from(4_u32));
        plan.glyphset_gsub.insert(GlyphId::from(7_u32));

        let font =
            FontRef::new(include_bytes!("../test-data/fonts/AnekBangla-subset.ttf")).unwrap();
        let gdef = font.gdef().unwrap();

        let mut varidx_set = IntSet::empty();
        gdef.collect_variation_indices(&plan, &mut varidx_set);
        assert_eq!(varidx_set.len(), 3);
        assert!(varidx_set.contains(3));
        assert!(varidx_set.contains(5));
        assert!(varidx_set.contains(0));
    }

    #[test]
    fn test_collect_used_mark_sets() {
        let mut plan = Plan::default();
        plan.glyphset_gsub.insert(GlyphId::from(171_u32));

        let font = FontRef::new(include_bytes!("../test-data/fonts/Roboto-Regular.ttf")).unwrap();
        let gdef = font.gdef().unwrap();

        let mut used_mark_sets = IntSet::empty();
        gdef.collect_used_mark_sets(&plan, &mut used_mark_sets);
        assert_eq!(used_mark_sets.len(), 1);
        assert!(used_mark_sets.contains(0));
    }
}
