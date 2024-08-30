//! impl subset() for GPOS table

use crate::{NameIdClosure, Plan};
use write_fonts::read::tables::gpos::Gpos;

impl<'a> NameIdClosure for Gpos<'a> {
    //TODO: support instancing: collect from feature substitutes if exist
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Ok(feature_list) = self.feature_list() else {
            return;
        };
        for (i, feature_record) in feature_list.feature_records().iter().enumerate() {
            if !plan.gpos_features.contains_key(&(i as u16)) {
                continue;
            }
            let Ok(feature) = feature_record.feature(self.offset_data()) else {
                continue;
            };
            feature.collect_name_ids(plan);
        }
    }
}
