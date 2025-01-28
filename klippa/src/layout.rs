//! impl subset() for layout common tables

use crate::{
    serialize::{SerializeErrorFlags, Serializer},
    CollectVaritionaIndices, NameIdClosure, Plan, SubsetTable,
};
use fnv::FnvHashMap;
use write_fonts::read::{
    collections::IntSet,
    tables::layout::{
        CharacterVariantParams, DeltaFormat, Device, DeviceOrVariationIndex, Feature,
        FeatureParams, SizeParams, StylisticSetParams, VariationIndex,
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

impl SubsetTable<'_> for DeviceOrVariationIndex<'_> {
    type ArgsForSubset = FnvHashMap<u32, (u32, i32)>;

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: &FnvHashMap<u32, (u32, i32)>,
    ) -> Result<(), SerializeErrorFlags> {
        match self {
            Self::Device(item) => item.subset(plan, s, &()),
            Self::VariationIndex(item) => item.subset(plan, s, args),
        }
    }
}

impl SubsetTable<'_> for Device<'_> {
    type ArgsForSubset = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: &(),
    ) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl SubsetTable<'_> for VariationIndex<'_> {
    type ArgsForSubset = FnvHashMap<u32, (u32, i32)>;

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        args: &FnvHashMap<u32, (u32, i32)>,
    ) -> Result<(), SerializeErrorFlags> {
        let var_idx =
            ((self.delta_set_outer_index() as u32) << 16) + self.delta_set_inner_index() as u32;
        let Some((new_idx, _)) = args.get(&var_idx) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };

        s.embed(*new_idx)?;
        s.embed(self.delta_format()).map(|_| ())
    }
}

impl CollectVaritionaIndices for DeviceOrVariationIndex<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        match self {
            Self::Device(_item) => (),
            Self::VariationIndex(item) => item.collect_variation_indices(plan, varidx_set),
        }
    }
}

impl CollectVaritionaIndices for VariationIndex<'_> {
    fn collect_variation_indices(&self, _plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if self.delta_format() == DeltaFormat::VariationIndex {
            let var_idx =
                ((self.delta_set_outer_index() as u32) << 16) + self.delta_set_inner_index() as u32;
            varidx_set.insert(var_idx);
        }
    }
}
