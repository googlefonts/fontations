//! impl subset() for GSUB table

use crate::{NameIdClosure, Plan};
use write_fonts::read::tables::gsub::Gsub;

impl NameIdClosure for Gsub<'_> {
    //TODO: support instancing: collect from feature substitutes if exist
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Ok(feature_list) = self.feature_list() else {
            return;
        };
        for (i, feature_record) in feature_list.feature_records().iter().enumerate() {
            if !plan.gsub_features.contains_key(&(i as u16)) {
                continue;
            }
            let Ok(feature) = feature_record.feature(feature_list.offset_data()) else {
                continue;
            };
            feature.collect_name_ids(plan);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{types::NameId, FontRef, TableProvider};
    #[test]
    fn test_nameid_closure() {
        let mut plan = Plan::default();
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoSansOriya-subset.ttf"
        ))
        .unwrap();
        let gsub = font.gsub().unwrap();
        gsub.collect_name_ids(&mut plan);
        assert!(plan.name_ids.is_empty());

        plan.gsub_features.insert(2, 1);
        gsub.collect_name_ids(&mut plan);
        assert_eq!(plan.name_ids.len(), 1);
        assert!(plan.name_ids.contains(NameId::new(257)));
    }
}
