//! impl subset() for hmtx

use crate::{
    offset::{SerializeCopy, SerializeSubset},
    offset_array::SubsetOffsetArray,
    serialize::{SerializeErrorFlags, Serializer},
    CollectVaritionaIndices, Plan, Subset, SubsetError, SubsetTable,
};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::base::{
            Axis, Base, BaseCoord, BaseCoordFormat1, BaseCoordFormat2, BaseCoordFormat3,
            BaseLangSysRecord, BaseScript, BaseScriptList, BaseScriptRecord, BaseValues,
            FeatMinMaxRecord, MinMax,
        },
        FontData, FontRef, TopLevelTable,
    },
    types::{GlyphId, MajorMinor, Offset16, Offset32},
    FontBuilder,
};

// reference: subset() for BASE in harfbuzz
// <https://github.com/harfbuzz/harfbuzz/blob/fc42cdd68df0ce710b507981184ade7bf1b164e6/src/hb-ot-layout-base-table.hh#L763>
impl Subset for Base<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        s.embed(self.version())
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

        //hAxis offset
        let haxis_offset_pos = s
            .embed(0_u16)
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

        if !self.horiz_axis_offset().is_null() {
            let Some(Ok(h_axis)) = self.horiz_axis() else {
                return Err(SubsetError::SubsetTableError(Base::TAG));
            };

            Offset16::serialize_subset(&h_axis, s, plan, &(), haxis_offset_pos)
                .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;
        }

        //vertAxis offset
        let vaxis_offset_pos = s
            .embed(0_u16)
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

        if !self.vert_axis_offset().is_null() {
            let Some(Ok(v_axis)) = self.vert_axis() else {
                return Err(SubsetError::SubsetTableError(Base::TAG));
            };

            Offset16::serialize_subset(&v_axis, s, plan, &(), vaxis_offset_pos)
                .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;
        }

        //itemVarStore offset
        match self.item_var_store() {
            Some(Ok(var_store)) => {
                let varstore_offset_pos = s
                    .embed(0_u32)
                    .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

                Offset32::serialize_subset(
                    &var_store,
                    s,
                    plan,
                    &plan.base_varstore_inner_maps,
                    varstore_offset_pos,
                )
                .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;
                Ok(())
            }
            None => {
                if self.version().minor > 0 {
                    s.copy_assign(0, MajorMinor::new(1, 0));
                }
                Ok(())
            }
            Some(Err(_)) => Err(SubsetError::SubsetTableError(Base::TAG)),
        }
    }
}

impl SubsetTable<'_> for Axis<'_> {
    type ArgsForSubset = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &(),
    ) -> Result<(), SerializeErrorFlags> {
        let base_taglist_offset_pos = s.embed(0_u16)?;
        let base_scriptlist_offset_pos = s.embed(0_u16)?;

        if !self.base_tag_list_offset().is_null() {
            let Some(Ok(base_taglist)) = self.base_tag_list() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };
            Offset16::serialize_copy(&base_taglist, s, base_taglist_offset_pos)?;
        }

        let Ok(base_scriptlist) = self.base_script_list() else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
        };
        Offset16::serialize_subset(&base_scriptlist, s, plan, &(), base_scriptlist_offset_pos)
    }
}

impl SubsetTable<'_> for BaseScriptList<'_> {
    type ArgsForSubset = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &(),
    ) -> Result<(), SerializeErrorFlags> {
        let script_count_pos = s.embed(0_u16)?;
        let mut count: usize = 0;
        for script_record in self.base_script_records().iter() {
            let script_tag = script_record.base_script_tag();
            if !plan.layout_scripts.contains(script_tag) {
                continue;
            }

            script_record.subset(plan, s, &self.offset_data())?;
            count += 1;
        }
        s.check_assign::<u16>(
            script_count_pos,
            count,
            SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
        )
    }
}

impl<'a> SubsetTable<'a> for BaseScriptRecord {
    type ArgsForSubset = FontData<'a>;

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: &FontData,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.base_script_tag())?;
        let base_script_offset_pos = s.embed(0_u16)?;
        let Ok(base_script) = self.base_script(*data) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
        };
        Offset16::serialize_subset(&base_script, s, plan, &(), base_script_offset_pos)
    }
}

impl SubsetTable<'_> for BaseScript<'_> {
    type ArgsForSubset = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let base_values_offset_pos = s.embed(0_u16)?;
        if !self.base_values_offset().is_null() {
            let Some(Ok(base_value)) = self.base_values() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };
            Offset16::serialize_subset(&base_value, s, plan, &(), base_values_offset_pos)?;
        }

        let default_min_max_offset_pos = s.embed(0_u16)?;
        if !self.default_min_max_offset().is_null() {
            let Some(Ok(default_min_max)) = self.default_min_max() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };
            Offset16::serialize_subset(&default_min_max, s, plan, &(), default_min_max_offset_pos)?;
        }

        s.embed(self.base_lang_sys_count())?;

        for record in self.base_lang_sys_records().iter() {
            record.subset(plan, s, &self.offset_data())?;
        }
        Ok(())
    }
}

impl SubsetTable<'_> for BaseValues<'_> {
    type ArgsForSubset = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.default_baseline_index())?;
        // base_coord count
        let mut base_coord_count: u16 = 0;
        let count_pos = s.embed(base_coord_count)?;

        let org_num_base_coords = self.base_coord_count() as usize;
        let base_coords = self.base_coords();
        for idx in 0..org_num_base_coords {
            base_coords.subset_offset(idx, s, plan, &())?;
            base_coord_count += 1;
        }
        s.copy_assign(count_pos, base_coord_count);
        Ok(())
    }
}

impl SubsetTable<'_> for MinMax<'_> {
    type ArgsForSubset = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let min_coord_offset_pos = s.embed(0_u16)?;
        if !self.min_coord_offset().is_null() {
            let Some(Ok(min_coord)) = self.min_coord() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };
            Offset16::serialize_subset(&min_coord, s, plan, &(), min_coord_offset_pos)?;
        }

        let max_coord_offset_pos = s.embed(0_u16)?;
        if !self.max_coord_offset().is_null() {
            let Some(Ok(max_coord)) = self.max_coord() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };
            Offset16::serialize_subset(&max_coord, s, plan, &(), max_coord_offset_pos)?;
        }

        let feat_min_max_count_pos = s.embed(0_u16)?;
        let mut count: u16 = 0;
        for record in self.feat_min_max_records().iter() {
            let feature_tag = record.feature_table_tag();
            if !plan.layout_features.contains(feature_tag) {
                continue;
            }
            record.subset(plan, s, &self.offset_data())?;
            count += 1;
        }
        s.copy_assign(feat_min_max_count_pos, count);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for FeatMinMaxRecord {
    type ArgsForSubset = FontData<'a>;

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: &FontData,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.feature_table_tag())?;

        let min_coord_offset_pos = s.embed(0_u16)?;
        if !self.min_coord_offset().is_null() {
            let Some(Ok(min_coord)) = self.min_coord(*data) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };
            Offset16::serialize_subset(&min_coord, s, plan, &(), min_coord_offset_pos)?;
        }

        let max_coord_offset_pos = s.embed(0_u16)?;
        if !self.max_coord_offset().is_null() {
            let Some(Ok(max_coord)) = self.max_coord(*data) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };
            Offset16::serialize_subset(&max_coord, s, plan, &(), max_coord_offset_pos)?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for BaseLangSysRecord {
    type ArgsForSubset = FontData<'a>;

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: &FontData,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.base_lang_sys_tag())?;

        let min_max_offset_pos = s.embed(0_u16)?;
        let Ok(min_max) = self.min_max(*data) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
        };
        Offset16::serialize_subset(&min_max, s, plan, &(), min_max_offset_pos)
    }
}

impl SubsetTable<'_> for BaseCoord<'_> {
    type ArgsForSubset = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s, &()),
            Self::Format2(item) => item.subset(plan, s, &()),
            Self::Format3(item) => item.subset(plan, s, &()),
        }
    }
}

impl SubsetTable<'_> for BaseCoordFormat1<'_> {
    type ArgsForSubset = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: &Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl SubsetTable<'_> for BaseCoordFormat2<'_> {
    type ArgsForSubset = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes())?;
        let Some(new_gid) = plan.glyph_map.get(&GlyphId::from(self.reference_glyph())) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        s.check_assign::<u16>(
            4,
            new_gid.to_u32() as usize,
            SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
        )
    }
}

impl SubsetTable<'_> for BaseCoordFormat3<'_> {
    type ArgsForSubset = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: &Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.base_coord_format())?;
        s.embed(self.coordinate())?;

        let device_offset_pos = s.embed(0_u16)?;
        if !self.device_offset().is_null() {
            let Some(Ok(device)) = self.device() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            };

            Offset16::serialize_subset(
                &device,
                s,
                plan,
                &plan.base_varidx_delta_map,
                device_offset_pos,
            )?;
        }
        Ok(())
    }
}

impl CollectVaritionaIndices for Base<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Some(Ok(h_axis)) = self.horiz_axis() {
            h_axis.collect_variation_indices(plan, varidx_set);
        }

        if let Some(Ok(v_axis)) = self.vert_axis() {
            v_axis.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVaritionaIndices for Axis<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Ok(base_scriptlist) = self.base_script_list() else {
            return;
        };
        base_scriptlist.collect_variation_indices(plan, varidx_set);
    }
}

impl CollectVaritionaIndices for BaseScriptList<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        for script_record in self.base_script_records().iter() {
            let script_tag = script_record.base_script_tag();
            if !plan.layout_scripts.contains(script_tag) {
                continue;
            }

            let Ok(base_script) = script_record.base_script(self.offset_data()) else {
                return;
            };
            base_script.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVaritionaIndices for BaseScript<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Some(Ok(base_values)) = self.base_values() {
            base_values.collect_variation_indices(plan, varidx_set);
        }

        if let Some(Ok(default_min_max)) = self.default_min_max() {
            default_min_max.collect_variation_indices(plan, varidx_set);
        }

        for record in self.base_lang_sys_records().iter() {
            if let Ok(min_max) = record.min_max(self.offset_data()) {
                min_max.collect_variation_indices(plan, varidx_set);
            }
        }
    }
}

impl CollectVaritionaIndices for BaseValues<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        for base_coord in self.base_coords().iter().flatten() {
            base_coord.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVaritionaIndices for MinMax<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Some(Ok(min_coord)) = self.min_coord() {
            min_coord.collect_variation_indices(plan, varidx_set);
        }

        if let Some(Ok(max_coord)) = self.max_coord() {
            max_coord.collect_variation_indices(plan, varidx_set);
        }

        for record in self.feat_min_max_records().iter() {
            let feature_tag = record.feature_table_tag();
            if !plan.layout_features.contains(feature_tag) {
                continue;
            }

            if let Some(Ok(min_coord)) = record.min_coord(self.offset_data()) {
                min_coord.collect_variation_indices(plan, varidx_set);
            }

            if let Some(Ok(max_coord)) = record.max_coord(self.offset_data()) {
                max_coord.collect_variation_indices(plan, varidx_set);
            }
        }
    }
}

impl CollectVaritionaIndices for BaseCoord<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Self::Format3(item) = self {
            item.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVaritionaIndices for BaseCoordFormat3<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Some(Ok(device)) = self.device() else {
            return;
        };
        device.collect_variation_indices(plan, varidx_set);
    }
}
