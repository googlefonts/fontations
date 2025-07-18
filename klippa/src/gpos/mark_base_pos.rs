//! impl subset() for MarkBasePos subtable
use crate::{gpos::mark_array::collect_mark_record_varidx, CollectVariationIndices, Plan};
use write_fonts::read::{collections::IntSet, tables::gpos::MarkBasePosFormat1, types::GlyphId};

impl CollectVariationIndices for MarkBasePosFormat1<'_> {
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

        let mut retained_mark_classes = IntSet::empty();
        for i in mark_coverage
            .iter()
            .enumerate()
            .filter(|&(_i, g)| glyph_set.contains(GlyphId::from(g)))
            .map(|(i, _)| i)
        {
            let Some(mark_record) = mark_records.get(i) else {
                return;
            };
            let class = mark_record.mark_class();
            collect_mark_record_varidx(mark_record, plan, varidx_set, mark_array_data);
            retained_mark_classes.insert(class);
        }

        let Ok(base_coverage) = self.base_coverage() else {
            return;
        };
        let Ok(base_array) = self.base_array() else {
            return;
        };
        let base_array_data = base_array.offset_data();
        let base_records = base_array.base_records();
        for i in base_coverage
            .iter()
            .enumerate()
            .filter(|&(_, g)| glyph_set.contains(GlyphId::from(g)))
            .map(|(i, _)| i)
        {
            let Ok(base_record) = base_records.get(i) else {
                return;
            };

            let base_anchors = base_record.base_anchors(base_array_data);
            for j in retained_mark_classes.iter() {
                if let Some(Ok(anchor)) = base_anchors.get(j as usize) {
                    anchor.collect_variation_indices(plan, varidx_set);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{FontRef, TableProvider};

    #[test]
    fn test_collect_variation_indices_markbasepos() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!(
            "../../test-data/fonts/RobotoFlex-Variable.ttf"
        ))
        .unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(1).unwrap();

        let PositionSubtables::MarkToBase(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let pairpos_table = sub_tables.get(0).unwrap();
        let mut plan = Plan::default();

        plan.glyphset_gsub.insert(GlyphId::from(36_u32));
        plan.glyphset_gsub.insert(GlyphId::from(390_u32));
        plan.glyphset_gsub.insert(GlyphId::from(405_u32));

        let mut varidx_set = IntSet::empty();
        pairpos_table.collect_variation_indices(&plan, &mut varidx_set);
        assert_eq!(varidx_set.len(), 5);
        assert!(varidx_set.contains(0x160004_u32));
        assert!(varidx_set.contains(0x110002_u32));
        assert!(varidx_set.contains(0x82002a_u32));
        assert!(varidx_set.contains(0x820044_u32));
        assert!(varidx_set.contains(0x850013_u32));
    }
}
