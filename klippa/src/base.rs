//! impl subset() for hmtx

use crate::{
    offset::{SerializeCopy, SerializeSubset},
    serialize::{SerializeErrorFlags, Serializer},
    Plan, Subset, SubsetError, SubsetTable, SubsetTableWithArgs, SubsetTableWithFontData,
};
use skrifa::raw::{tables::base::MinMax, FontData};
use write_fonts::types::{GlyphId, Offset16};
use write_fonts::{
    read::{
        tables::{
            base::{
                Axis, Base, BaseCoord, BaseCoordFormat1, BaseCoordFormat2, BaseCoordFormat3,
                BaseLangSysRecord, BaseScript, BaseScriptList, BaseScriptRecord, BaseValues,
                FeatMinMaxRecord,
            },
            layout::{Device, DeviceOrVariationIndex, VariationIndex},
        },
        FontRef, TopLevelTable,
    },
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

        //vertAxis offset
        let haxis_offset_pos = s
            .embed(0_u16)
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

        //itemVarStore offset
        match self.item_var_store() {
            Some(Ok(var_store)) => {
                let varstore_offset_pos = s
                    .embed(0_u32)
                    .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;
                Ok(())
            }
            None => Ok(()),
            Some(Err(_)) => Err(SubsetError::SubsetTableError(Base::TAG)),
        }
    }
}

impl SubsetTable for Axis<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        let base_taglist_offset_pos = s.embed(0_u16)?;
        let base_scriptlist_offset_pos = s.embed(0_u16)?;

        if !self.base_tag_list_offset().is_null() {
            let Some(Ok(base_taglist)) = self.base_tag_list() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            Offset16::serialize_copy(&base_taglist, s, base_taglist_offset_pos)?;
        }

        let Ok(base_scriptlist) = self.base_script_list() else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        Offset16::serialize_subset(&base_scriptlist, s, plan, base_scriptlist_offset_pos)
    }
}

impl SubsetTable for BaseScriptList<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        let script_count_pos = s.embed(0_u16)?;
        let mut count: usize = 0;
        for script_record in self.base_script_records().iter() {
            let script_tag = script_record.base_script_tag();
            if !plan.layout_scripts.contains(script_tag) {
                continue;
            }

            script_record.subset_with_font_data(plan, s, self.offset_data())?;
            count += 1;
        }
        s.check_assign::<u16>(
            script_count_pos,
            count,
            SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
        )
    }
}

impl SubsetTableWithFontData for BaseScriptRecord {
    fn subset_with_font_data<'a>(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: FontData<'a>,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.base_script_tag())?;
        let base_script_offset_pos = s.embed(0_u16)?;
        let Ok(base_script) = self.base_script(data) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        Offset16::serialize_subset(&base_script, s, plan, base_script_offset_pos)
    }
}

impl SubsetTable for BaseScript<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        let base_values_offset_pos = s.embed(0_u16)?;
        if !self.base_values_offset().is_null() {
            let Some(Ok(base_value)) = self.base_values() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            Offset16::serialize_subset(&base_value, s, plan, base_values_offset_pos)?;
        }

        let default_min_max_offset_pos = s.embed(0_u16)?;
        if !self.default_min_max_offset().is_null() {
            let Some(Ok(default_min_max)) = self.default_min_max() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            Offset16::serialize_subset(&default_min_max, s, plan, default_min_max_offset_pos)?;
        }

        s.embed(self.base_lang_sys_count())?;

        for record in self.base_lang_sys_records().iter() {
            record.subset_with_font_data(plan, s, self.offset_data())?;
        }
        Ok(())
    }
}

impl SubsetTable for BaseValues<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        s.embed(self.default_baseline_index())?;
        subset_offset_array();
        Ok(())
    }
}

impl SubsetTable for MinMax<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        let min_coord_offset_pos = s.embed(0_u16)?;
        if !self.min_coord_offset().is_null() {
            let Some(Ok(min_coord)) = self.min_coord() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            Offset16::serialize_subset(&min_coord, s, plan, min_coord_offset_pos)?;
        }

        let max_coord_offset_pos = s.embed(0_u16)?;
        if !self.max_coord_offset().is_null() {
            let Some(Ok(max_coord)) = self.max_coord() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            Offset16::serialize_subset(&max_coord, s, plan, max_coord_offset_pos)?;
        }

        let feat_min_max_count_pos = s.embed(0_u16)?;
        let mut count: u16 = 0;
        for record in self.feat_min_max_records().iter() {
            let feature_tag = record.feature_table_tag();
            if !plan.layout_features.contains(feature_tag) {
                continue;
            }
            record.subset_with_font_data(plan, s, self.offset_data())?;
            count += 1;
        }
        s.copy_assign(feat_min_max_count_pos, count);
        Ok(())
    }
}

impl SubsetTableWithFontData for FeatMinMaxRecord {
    fn subset_with_font_data<'a>(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: FontData<'a>,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.feature_table_tag())?;

        let min_coord_offset_pos = s.embed(0_u16)?;
        if !self.min_coord_offset().is_null() {
            let Some(Ok(min_coord)) = self.min_coord(data) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            Offset16::serialize_subset(&min_coord, s, plan, min_coord_offset_pos)?;
        }

        let max_coord_offset_pos = s.embed(0_u16)?;
        if !self.max_coord_offset().is_null() {
            let Some(Ok(max_coord)) = self.max_coord(data) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            Offset16::serialize_subset(&max_coord, s, plan, max_coord_offset_pos)?;
        }
        Ok(())
    }
}

impl SubsetTableWithFontData for BaseLangSysRecord {
    fn subset_with_font_data<'a>(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: FontData<'a>,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.base_lang_sys_tag())?;

        let min_max_offset_pos = s.embed(0_u16)?;
        let Ok(min_max) = self.min_max(data) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        Offset16::serialize_subset(&min_max, s, plan, min_max_offset_pos)
    }
}

impl SubsetTable for BaseCoord<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s),
            Self::Format2(item) => item.subset(plan, s),
            Self::Format3(item) => item.subset(plan, s),
        }
    }
}

impl SubsetTable for BaseCoordFormat1<'_> {
    fn subset(&self, _plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.offset_data().as_bytes()).map(|_| ())
    }
}

impl SubsetTable for BaseCoordFormat2<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.offset_data().as_bytes())?;
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

impl SubsetTable for BaseCoordFormat3<'_> {
    fn subset(&self, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
        s.embed(self.base_coord_format())?;
        s.embed(self.coordinate())?;

        let device_offset_pos = s.embed(0_u16)?;
        if !self.device_offset().is_null() {
            let Some(Ok(device)) = self.device() else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };

            Offset16::serialize_subset_with_args(
                &device,
                s,
                plan,
                device_offset_pos,
                &plan.base_varidx_delta_map,
            )?;
        }
        Ok(())
    }
}