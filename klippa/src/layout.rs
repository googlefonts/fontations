//! impl subset() for layout common tables

use crate::{NameIdClosure, Plan};
use write_fonts::read::{
    tables::layout::{
        CharacterVariantParams, Feature, FeatureParams, SizeParams, StylisticSetParams,
    },
    types::NameId,
};

impl NameIdClosure for StylisticSetParams<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        plan.name_ids.insert(self.ui_name_id());
    }
}

impl NameIdClosure for SizeParams<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        plan.name_ids.insert(NameId::new(self.name_entry()));
    }
}

impl NameIdClosure for CharacterVariantParams<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        plan.name_ids.insert(self.feat_ui_label_name_id());
        plan.name_ids.insert(self.feat_ui_tooltip_text_name_id());
        plan.name_ids.insert(self.sample_text_name_id());

        let first_name_id = self.first_param_ui_label_name_id();
        let num_named_params = self.num_named_parameters();
        if first_name_id == NameId::COPYRIGHT_NOTICE
            || num_named_params == 0
            || num_named_params >= 0x7FFF
        {
            return;
        }

        let last_name_id = first_name_id.to_u16() as u32 + num_named_params as u32 - 1;
        plan.name_ids
            .insert_range(first_name_id..=NameId::new(last_name_id as u16));
    }
}

impl NameIdClosure for Feature<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Some(Ok(feature_params)) = self.feature_params() else {
            return;
        };
        match feature_params {
            FeatureParams::StylisticSet(table) => table.collect_name_ids(plan),
            FeatureParams::Size(table) => table.collect_name_ids(plan),
            FeatureParams::CharacterVariant(table) => table.collect_name_ids(plan),
        }
    }
}
