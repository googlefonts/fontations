//! impl subset() for MarkLigPos subtable
use crate::{
    gpos::mark_array::collect_mark_record_varidx, layout::intersected_coverage_indices,
    CollectVariationIndices, Plan,
};
use write_fonts::read::{collections::IntSet, tables::gpos::MarkLigPosFormat1};

impl CollectVariationIndices for MarkLigPosFormat1<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Ok(mark_coverage) = self.mark_coverage() else {
            return;
        };
        let Ok(mark_array) = self.mark_array() else {
            return;
        };

        let glyph_set = &plan.glyphset_gsub;
        let mark_array_data = mark_array.offset_data();
        let mark_records = mark_array.mark_records();

        let mark_record_idxes = intersected_coverage_indices(&mark_coverage, glyph_set);
        let mut retained_mark_classes = IntSet::empty();
        for i in mark_record_idxes.iter() {
            let Some(mark_record) = mark_records.get(i as usize) else {
                return;
            };
            let class = mark_record.mark_class();
            collect_mark_record_varidx(mark_record, plan, varidx_set, mark_array_data);
            retained_mark_classes.insert(class);
        }

        let Ok(lig_coverage) = self.ligature_coverage() else {
            return;
        };
        let Ok(lig_array) = self.ligature_array() else {
            return;
        };
        let lig_attaches = lig_array.ligature_attaches();
        let lig_attach_idxes = intersected_coverage_indices(&lig_coverage, glyph_set);
        for i in lig_attach_idxes.iter() {
            let Ok(lig_attach) = lig_attaches.get(i as usize) else {
                return;
            };

            let lig_attach_data = lig_attach.offset_data();
            for component in lig_attach.component_records().iter() {
                let Ok(component) = component else {
                    return;
                };

                let lig_anchors = component.ligature_anchors(lig_attach_data);
                for j in retained_mark_classes.iter() {
                    if let Some(Ok(anchor)) = lig_anchors.get(j as usize) {
                        anchor.collect_variation_indices(plan, varidx_set);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{types::GlyphId, FontRef, TableProvider};

    #[test]
    fn test_collect_variation_indices_markligpos() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!(
            "../../test-data/fonts/Comfortaa-Regular-new.ttf"
        ))
        .unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(2).unwrap();

        let PositionSubtables::MarkToLig(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let markligpos_table = sub_tables.get(0).unwrap();
        let mut plan = Plan::default();

        plan.glyphset_gsub.insert(GlyphId::from(396_u32));
        plan.glyphset_gsub.insert(GlyphId::from(839_u32));

        let mut varidx_set = IntSet::empty();
        markligpos_table.collect_variation_indices(&plan, &mut varidx_set);
        assert_eq!(varidx_set.len(), 3);
        assert!(varidx_set.contains(0x20065_u32));
        assert!(varidx_set.contains(0x20062_u32));
        assert!(varidx_set.contains(0x2004c_u32));
    }
}
