//! impl subset() for hmtx

use crate::{
    layout::OpsWalk,
    offset::{SerializeCopy, SerializeSubset},
    offset_array::IterNullableHelper,
    serialize::{SerializeErrorFlags, SerializeResultEmpty, Serializer},
    CollectVariationIndices, Plan, Subset, SubsetError, SubsetTable,
};
use read_fonts::{
    collections::IntSet,
    tables::base::{
        Axis, Base, BaseCoord, BaseCoordFormat1, BaseCoordFormat2, BaseCoordFormat3,
        BaseLangSysRecord, BaseScript, BaseScriptList, BaseScriptRecord, BaseValues,
        FeatMinMaxRecord, MinMax,
    },
    types::{FixedSize, GlyphId, MajorMinor, Offset16, Offset32},
    FontData, FontRef, MinByteRange, TopLevelTable,
};
use write_fonts::FontBuilder;

/// Counts a table that subsetting is about to read against the budget of the
/// BASE table, and fails the table once the budget is spent.
fn visit<'a>(s: &mut Serializer, table: &impl MinByteRange<'a>) -> Result<(), SerializeErrorFlags> {
    if s.layout_walk().visit(table) {
        Ok(())
    } else {
        Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))
    }
}

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
        // Nothing stops all script records from pointing at one BaseScript and
        // all of its language system records at one MinMax, which is then read
        // once for every path that leads to it. So the BaseScript, BaseValues
        // and MinMax tables count as often as they are reached, against what
        // HarfBuzz's sanitizer may read of the whole table. The coordinates
        // they lead to are a few bytes each, so the offsets that lead to them
        // are count enough.
        s.set_layout_walk(OpsWalk::new(self.offset_data()));

        //hAxis offset
        let haxis_offset_pos = s
            .embed(0_u16)
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

        if let Some(h_axis) = self
            .horiz_axis()
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?
        {
            Offset16::serialize_subset(&h_axis, s, plan, (), haxis_offset_pos)
                .is_empty()
                .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;
        }

        //vertAxis offset
        let vaxis_offset_pos = s
            .embed(0_u16)
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

        if let Some(v_axis) = self
            .vert_axis()
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?
        {
            Offset16::serialize_subset(&v_axis, s, plan, (), vaxis_offset_pos)
                .is_empty()
                .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;
        }

        //itemVarStore offset
        if let Some(var_store) = self
            .item_var_store()
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?
        {
            let varstore_offset_pos = s
                .embed(0_u32)
                .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;

            Offset32::serialize_subset(
                &var_store,
                s,
                plan,
                (&plan.base_varstore_inner_maps, false),
                varstore_offset_pos,
            )
            .is_empty()
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?;
        } else if self.version().minor > 0 {
            s.copy_assign(0, MajorMinor::new(1, 0));
        }
        Ok(())
    }
}

impl SubsetTable<'_> for Axis<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: (),
    ) -> Result<(), SerializeErrorFlags> {
        let base_taglist_offset_pos = s.embed(0_u16)?;
        let base_scriptlist_offset_pos = s.embed(0_u16)?;

        if let Some(base_taglist) = self
            .base_tag_list()
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            Offset16::serialize_copy(&base_taglist, s, base_taglist_offset_pos)?;
        }

        if self.base_script_list_offset().is_null() {
            return Ok(());
        }
        let base_scriptlist = self
            .base_script_list()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        Offset16::serialize_subset(&base_scriptlist, s, plan, (), base_scriptlist_offset_pos)
    }
}

impl SubsetTable<'_> for BaseScriptList<'_> {
    type ArgsForSubset = ();
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: (),
    ) -> Result<(), SerializeErrorFlags> {
        let script_count_pos = s.embed(0_u16)?;
        let mut count: usize = 0;
        for script_record in self.base_script_records().iter() {
            let script_tag = script_record.base_script_tag();
            if !plan.layout_scripts.contains(script_tag) {
                continue;
            }

            if !script_record
                .subset(plan, s, self.offset_data())
                .is_empty()?
            {
                count += 1;
            }
        }

        if count != 0 {
            s.check_assign::<u16>(
                script_count_pos,
                count,
                SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
            )?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for BaseScriptRecord {
    type ArgsForSubset = FontData<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: FontData,
    ) -> Result<(), SerializeErrorFlags> {
        if self.base_script_offset().is_null() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.embed(self.base_script_tag())?;
        let base_script_offset_pos = s.embed(0_u16)?;
        let base_script = self
            .base_script(data)
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        visit(s, &base_script)?;
        Offset16::serialize_subset(&base_script, s, plan, (), base_script_offset_pos)
    }
}

impl SubsetTable<'_> for BaseScript<'_> {
    type ArgsForSubset = ();
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let base_values_offset_pos = s.embed(0_u16)?;
        if let Some(base_value) = self
            .base_values()
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            visit(s, &base_value)?;
            Offset16::serialize_subset(&base_value, s, plan, (), base_values_offset_pos)?;
        }

        let default_min_max_offset_pos = s.embed(0_u16)?;
        if let Some(default_min_max) = self
            .default_min_max()
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            visit(s, &default_min_max)?;
            Offset16::serialize_subset(&default_min_max, s, plan, (), default_min_max_offset_pos)
                .is_empty()?;
        }

        let base_lang_sys_count_pos = s.embed(0_u16)?;
        if self.base_lang_sys_count() == 0 {
            return Ok(());
        }

        let mut count = 0_u16;
        for record in self.base_lang_sys_records().iter() {
            if !record.subset(plan, s, self.offset_data()).is_empty()? {
                count += 1;
            }
        }

        if count != 0 {
            s.copy_assign(base_lang_sys_count_pos, count);
        }
        Ok(())
    }
}

impl SubsetTable<'_> for BaseValues<'_> {
    type ArgsForSubset = ();
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.default_baseline_index())?;
        let base_coord_count = self.base_coord_count();
        s.embed(base_coord_count)?;

        if base_coord_count == 0 {
            return Ok(());
        }

        let base_coords = self.base_coords();
        let pos_start =
            s.allocate_size(base_coord_count as usize * Offset16::RAW_BYTE_LEN, true)?;
        for (idx, base_coord) in base_coords.iter_as_nullable().enumerate() {
            let Some(base_coord) = base_coord
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            else {
                continue;
            };
            let offset_pos = pos_start + idx * Offset16::RAW_BYTE_LEN;
            Offset16::serialize_subset(&base_coord, s, plan, (), offset_pos).is_empty()?;
        }
        Ok(())
    }
}

impl SubsetTable<'_> for MinMax<'_> {
    type ArgsForSubset = ();
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let snap = s.snapshot();
        let mut non_empty = false;
        let min_coord_offset_pos = s.embed(0_u16)?;
        if let Some(min_coord) = self
            .min_coord()
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            non_empty |= !Offset16::serialize_subset(&min_coord, s, plan, (), min_coord_offset_pos)
                .is_empty()?;
        }

        let max_coord_offset_pos = s.embed(0_u16)?;
        if let Some(max_coord) = self
            .max_coord()
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            non_empty |= !Offset16::serialize_subset(&max_coord, s, plan, (), max_coord_offset_pos)
                .is_empty()?;
        }

        let feat_min_max_count_pos = s.embed(0_u16)?;
        let mut count: u16 = 0;
        for record in self.feat_min_max_records().iter() {
            let feature_tag = record.feature_table_tag();
            if !plan.layout_features.contains(feature_tag) {
                continue;
            }
            if !record.subset(plan, s, self.offset_data()).is_empty()? {
                count += 1;
            }
        }

        if count != 0 {
            non_empty = true;
            s.copy_assign(feat_min_max_count_pos, count);
        }

        if !non_empty {
            s.revert_snapshot(snap);
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for FeatMinMaxRecord {
    type ArgsForSubset = FontData<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: FontData,
    ) -> Result<(), SerializeErrorFlags> {
        if self.max_coord_offset().is_null() && self.min_coord_offset().is_null() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.embed(self.feature_table_tag())?;

        let snap = s.snapshot();
        let min_coord_offset_pos = s.embed(0_u16)?;
        let mut non_empty = false;
        if let Some(min_coord) = self
            .min_coord(data)
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            non_empty |= !Offset16::serialize_subset(&min_coord, s, plan, (), min_coord_offset_pos)
                .is_empty()?;
        }

        let max_coord_offset_pos = s.embed(0_u16)?;
        if let Some(max_coord) = self
            .max_coord(data)
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            non_empty |= !Offset16::serialize_subset(&max_coord, s, plan, (), max_coord_offset_pos)
                .is_empty()?;
        }

        if !non_empty {
            s.revert_snapshot(snap);
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for BaseLangSysRecord {
    type ArgsForSubset = FontData<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        data: FontData,
    ) -> Result<(), SerializeErrorFlags> {
        if self.min_max_offset().is_null() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.embed(self.base_lang_sys_tag())?;

        let min_max_offset_pos = s.embed(0_u16)?;
        let Ok(min_max) = self.min_max(data) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
        };
        visit(s, &min_max)?;
        Offset16::serialize_subset(&min_max, s, plan, (), min_max_offset_pos)
    }
}

impl SubsetTable<'_> for BaseCoord<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s, ()),
            Self::Format2(item) => item.subset(plan, s, ()),
            Self::Format3(item) => item.subset(plan, s, ()),
        }
    }
}

impl SubsetTable<'_> for BaseCoordFormat1<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl SubsetTable<'_> for BaseCoordFormat2<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes())?;
        let Some(new_gid) = plan.glyph_map.get(&GlyphId::from(self.reference_glyph())) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
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
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.base_coord_format())?;
        s.embed(self.coordinate())?;

        let device_offset_pos = s.embed(0_u16)?;
        if let Some(device) = self
            .device()
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            Offset16::serialize_subset(
                &device,
                s,
                plan,
                &plan.base_varidx_delta_map,
                device_offset_pos,
            )
            .is_empty()?;
        }
        Ok(())
    }
}

impl CollectVariationIndices for Base<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        // Counts what it reads as subsetting does, which then runs out of
        // budget wherever this walk does.
        let mut walk = OpsWalk::new(self.offset_data());
        for axis in [self.horiz_axis(), self.vert_axis()] {
            if let Some(Ok(axis)) = axis {
                if let Ok(base_scriptlist) = axis.base_script_list() {
                    collect_script_list_var_indices(&base_scriptlist, plan, varidx_set, &mut walk);
                }
            }
        }
    }
}

fn collect_script_list_var_indices(
    script_list: &BaseScriptList,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
    walk: &mut OpsWalk,
) {
    for script_record in script_list.base_script_records().iter() {
        let script_tag = script_record.base_script_tag();
        if !plan.layout_scripts.contains(script_tag) || script_record.base_script_offset().is_null()
        {
            continue;
        }

        let Ok(base_script) = script_record.base_script(script_list.offset_data()) else {
            return;
        };
        if walk.visit(&base_script) {
            collect_script_var_indices(&base_script, plan, varidx_set, walk);
        }
    }
}

fn collect_script_var_indices(
    script: &BaseScript,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
    walk: &mut OpsWalk,
) {
    if let Some(Ok(base_values)) = script.base_values() {
        if walk.visit(&base_values) {
            base_values.collect_variation_indices(plan, varidx_set);
        }
    }

    if let Some(Ok(default_min_max)) = script.default_min_max() {
        if walk.visit(&default_min_max) {
            default_min_max.collect_variation_indices(plan, varidx_set);
        }
    }

    for record in script.base_lang_sys_records().iter() {
        if record.min_max_offset().is_null() {
            continue;
        }
        if let Ok(min_max) = record.min_max(script.offset_data()) {
            if walk.visit(&min_max) {
                min_max.collect_variation_indices(plan, varidx_set);
            }
        }
    }
}

impl CollectVariationIndices for BaseValues<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        for base_coord in self.base_coords().iter_as_nullable().flatten().flatten() {
            base_coord.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for MinMax<'_> {
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

impl CollectVariationIndices for BaseCoord<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Self::Format3(item) = self {
            item.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for BaseCoordFormat3<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Some(Ok(device)) = self.device() else {
            return;
        };
        device.collect_variation_indices(plan, varidx_set);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use font_test_data::bebuffer::BeBuffer;
    use read_fonts::{types::Tag, FontRead};

    // One BaseScript and one MinMax, each the target of the given number of
    // offsets.
    fn base_with_shared_tables(
        scripts: u16,
        lang_systems: u16,
        features: u16,
        min_coord: &[u16],
    ) -> Vec<u8> {
        let mut base = BeBuffer::new()
            .extend([1_u16, 0, 8, 0])
            // Axis
            .extend([0_u16, 4])
            // BaseScriptList
            .push(scripts);
        for _ in 0..scripts {
            base = base.push(Tag::new(b"DFLT")).push(2 + 6 * scripts);
        }
        // BaseScript
        base = base.extend([0, 0, lang_systems]);
        for _ in 0..lang_systems {
            base = base.push(Tag::new(b"ENG ")).push(6 + 6 * lang_systems);
        }
        // MinMax
        base = base.extend([6 + 8 * features, 0, features]);
        for _ in 0..features {
            base = base.push(Tag::new(b"zzzz")).extend([0_u16, 0]);
        }
        base.extend(min_coord.iter().copied()).to_vec()
    }

    // The subset BASE, or whether the serializer is in error.
    fn subset(data: &[u8]) -> Result<Vec<u8>, bool> {
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        let base = Base::read(FontData::new(data)).unwrap();
        let mut s = Serializer::new(1024 * 1024).unwrap();
        s.start_serialize().unwrap();
        let ret = base.subset(&plan(), &font, &mut s, &mut FontBuilder::new());
        s.end_serialize();
        match ret {
            Ok(()) => Ok(s.copy_bytes()),
            Err(_) => Err(s.in_error()),
        }
    }

    fn plan() -> Plan {
        Plan {
            layout_scripts: IntSet::all(),
            ..Default::default()
        }
    }

    #[test]
    fn shared_tables_count_once_per_path() {
        // BaseCoordFormat1
        let coord = [1, 100];
        // n scripts lead to a BaseScript of 6 + 6n bytes and n MinMax tables of
        // 6 bytes each: 12n² + 6n bytes against 64 for each of the 30 + 12n.
        let data = base_with_shared_tables(65, 65, 0, &coord);
        let out = subset(&data).unwrap();
        let base = Base::read(FontData::new(&out)).unwrap();
        let script_list = base.horiz_axis().unwrap().unwrap().base_script_list();
        let script_list = script_list.unwrap();
        assert_eq!(script_list.base_script_count(), 65);
        let script = script_list.base_script_records()[64]
            .base_script(script_list.offset_data())
            .unwrap();
        assert_eq!(script.base_lang_sys_count(), 65);
        let record = script.base_lang_sys_records().last().unwrap();
        let min_max = record.min_max(script.offset_data()).unwrap();
        let min_coord = min_max.min_coord().unwrap().unwrap();
        assert_eq!(min_coord.coordinate(), 100);
        // the serializer shares what is the same
        assert_eq!(out, data);

        // 52668 bytes, against 52608
        let data = base_with_shared_tables(66, 66, 0, &coord);
        assert_eq!(subset(&data), Err(true));
    }

    #[test]
    fn shared_tables_over_budget_fail_to_subset() {
        // a billion feature records, without a budget
        let data = base_with_shared_tables(1000, 1000, 1000, &[1, 100]);
        assert_eq!(subset(&data), Err(true));
    }

    #[test]
    fn shared_tables_collect_variation_indices_within_budget() {
        // BaseCoordFormat3 with a VariationIndex
        let coord = [3, 100, 6, 1, 2, 0x8000];
        let data = base_with_shared_tables(2, 3, 4, &coord);
        let base = Base::read(FontData::new(&data)).unwrap();
        let mut varidx_set = IntSet::empty();
        base.collect_variation_indices(&plan(), &mut varidx_set);
        assert_eq!(varidx_set.iter().collect::<Vec<_>>(), [0x10002]);

        // a billion feature records, without a budget
        let data = base_with_shared_tables(1000, 1000, 1000, &coord);
        let base = Base::read(FontData::new(&data)).unwrap();
        let mut varidx_set = IntSet::empty();
        base.collect_variation_indices(&plan(), &mut varidx_set);
        assert!(varidx_set.iter().all(|idx| idx == 0x10002));
    }
}
