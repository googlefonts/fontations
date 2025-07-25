//! impl subset() for MarkMarkPos subtable
use crate::{gpos::mark_array::collect_mark_record_varidx, CollectVariationIndices, Plan};
use write_fonts::read::{collections::IntSet, tables::gpos::MarkMarkPosFormat1, types::GlyphId};

impl CollectVariationIndices for MarkMarkPosFormat1<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Ok(mark1_coverage) = self.mark1_coverage() else {
            return;
        };
        let Ok(mark1_array) = self.mark1_array() else {
            return;
        };

        let glyph_set = &plan.glyphset_gsub;
        let mark1_array_data = mark1_array.offset_data();
        let mark1_records = mark1_array.mark_records();

        let mut retained_mark_classes = IntSet::empty();
        for i in mark1_coverage
            .iter()
            .enumerate()
            .filter_map(|(i, g)| glyph_set.contains(GlyphId::from(g)).then_some(i))
        {
            let Some(mark1_record) = mark1_records.get(i) else {
                return;
            };
            let class = mark1_record.mark_class();
            collect_mark_record_varidx(mark1_record, plan, varidx_set, mark1_array_data);
            retained_mark_classes.insert(class);
        }

        let Ok(mark2_coverage) = self.mark2_coverage() else {
            return;
        };
        let Ok(mark2_array) = self.mark2_array() else {
            return;
        };
        let mark2_array_data = mark2_array.offset_data();
        let mark2_records = mark2_array.mark2_records();
        for i in mark2_coverage
            .iter()
            .enumerate()
            .filter_map(|(i, g)| glyph_set.contains(GlyphId::from(g)).then_some(i))
        {
            let Ok(mark2_record) = mark2_records.get(i) else {
                return;
            };
            let mark2_anchors = mark2_record.mark2_anchors(mark2_array_data);
            for j in retained_mark_classes.iter() {
                if let Some(Ok(anchor)) = mark2_anchors.get(j as usize) {
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
    fn test_collect_variation_indices_markmarkpos() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!(
            "../../test-data/fonts/RobotoFlex-Variable.ttf"
        ))
        .unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(2).unwrap();

        let PositionSubtables::MarkToMark(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let markmarkpos_table = sub_tables.get(0).unwrap();
        let mut plan = Plan::default();

        plan.glyphset_gsub.insert(GlyphId::from(408_u32));
        plan.glyphset_gsub.insert(GlyphId::from(583_u32));

        let mut varidx_set = IntSet::empty();
        markmarkpos_table.collect_variation_indices(&plan, &mut varidx_set);
        assert_eq!(varidx_set.len(), 5);
        assert!(varidx_set.contains(0x300000_u32));
        assert!(varidx_set.contains(0x6f001a_u32));
        assert!(varidx_set.contains(0x110002_u32));
        assert!(varidx_set.contains(0x220000_u32));
        assert!(varidx_set.contains(0x86002e_u32));
    }
}
