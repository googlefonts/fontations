//! impl subset() for STAT table

use crate::{
    offset::SerializeSubset, serialize::SerializeErrorFlags, variations::solver::Triple,
    NameIdClosure, Plan, Subset, SubsetError, SubsetTable,
};
use fnv::FnvHashMap;
use font_types::{NameId, Offset16, Offset32};
use skrifa::{
    raw::{
        tables::stat::{
            AxisRecord, AxisValue, AxisValueFormat1, AxisValueFormat2, AxisValueFormat3,
            AxisValueFormat4,
        },
        TopLevelTable,
    },
    Tag,
};
use write_fonts::read::tables::stat::Stat;

impl NameIdClosure for Stat<'_> {
    //TODO: support instancing
    fn collect_name_ids(&self, plan: &mut Plan) {
        if let Ok(axis_records) = self.design_axes() {
            plan.name_ids
                .extend_unsorted(axis_records.iter().map(|x| x.axis_name_id()));
        }

        if let Some(Ok(axis_values)) = self.offset_to_axis_values() {
            plan.name_ids
                .extend_unsorted(axis_values.axis_values().iter().filter_map(|x| match x {
                    Ok(axis_value) => Some(axis_value.value_name_id()),
                    _ => None,
                }));
        }
        if let Some(name_id) = self.elided_fallback_name_id() {
            plan.name_ids.insert(name_id);
        }
    }
}
impl Subset for Stat<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &write_fonts::read::FontRef,
        s: &mut crate::serialize::Serializer,
        _builder: &mut write_fonts::FontBuilder,
    ) -> Result<(), crate::SubsetError> {
        subset_stat(self, plan, s).map_err(|_| SubsetError::SubsetTableError(Stat::TAG))
    }
}

fn subset_stat(
    stat: &Stat<'_>,
    plan: &Plan,
    s: &mut crate::serialize::Serializer,
) -> Result<(), SerializeErrorFlags> {
    // Copy in the whole thing
    s.embed(stat.version())?;
    s.embed(stat.design_axis_size())?;
    s.embed(stat.design_axis_count())?;
    let design_axes_offset_pos = s
        .embed(0_u32)
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
    let design_axes = &stat
        .design_axes()
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
    Offset32::serialize_subset(design_axes, s, plan, (), design_axes_offset_pos)?;

    let axis_value_count_pos = s
        .embed(0_u16)
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
    let axis_value_array_offset_pos = s
        .embed(0_u32)
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
    s.embed(stat.elided_fallback_name_id().unwrap_or(NameId::new(0)))
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
    if let Some(Ok(subtable)) = stat.offset_to_axis_values() {
        let axis_values = subtable.axis_values().iter().flatten().collect::<Vec<_>>();
        if let Ok(count) = Offset32::serialize_subset(
            &axis_values,
            s,
            plan,
            design_axes,
            axis_value_array_offset_pos,
        ) {
            s.copy_assign(axis_value_count_pos, count as u16);
        }
    }
    Ok(())
}

impl<'a> SubsetTable<'a> for &'a [AxisRecord] {
    type ArgsForSubset = ();
    type Output = ();

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut crate::serialize::Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        for axis_record in self.iter() {
            s.embed(axis_record.axis_tag())?;
            s.embed(axis_record.axis_name_id())?;
            s.embed(axis_record.axis_ordering())?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for Vec<AxisValue<'a>> {
    type ArgsForSubset = &'a [AxisRecord];
    type Output = usize; // the count of retained axis values

    fn subset(
        &self,
        plan: &Plan,
        s: &mut crate::serialize::Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let mut count = 0;
        log::debug!("I found {} records", self.len());

        for axis_record in self.iter() {
            let snap = s.snapshot();
            let pos = s.embed(0_u16)?; // placeholder for offset
            match axis_record {
                AxisValue::Format1(t) => match Offset16::serialize_subset(t, s, plan, args, pos) {
                    Ok(true) => count += 1,
                    Ok(false) | Err(SerializeErrorFlags::SERIALIZE_ERROR_NONE) => {
                        s.revert_snapshot(snap)
                    }
                    Err(e) => return Err(e),
                },
                AxisValue::Format2(t) => match Offset16::serialize_subset(t, s, plan, args, pos) {
                    Ok(true) => count += 1,
                    Ok(false) | Err(SerializeErrorFlags::SERIALIZE_ERROR_NONE) => {
                        s.revert_snapshot(snap)
                    }
                    Err(e) => return Err(e),
                },
                AxisValue::Format3(t) => match Offset16::serialize_subset(t, s, plan, args, pos) {
                    Ok(true) => count += 1,
                    Ok(false) | Err(SerializeErrorFlags::SERIALIZE_ERROR_NONE) => {
                        s.revert_snapshot(snap)
                    }
                    Err(e) => return Err(e),
                },
                AxisValue::Format4(t) => match Offset16::serialize_subset(t, s, plan, args, pos) {
                    Ok(true) => count += 1,
                    Ok(false) | Err(SerializeErrorFlags::SERIALIZE_ERROR_NONE) => {
                        s.revert_snapshot(snap)
                    }
                    Err(e) => return Err(e),
                },
            }
        }
        Ok(count)
    }
}

fn axis_value_is_outside_axis_range(
    axis_tag: Tag,
    axis_value: f32,
    user_axes_location: &FnvHashMap<Tag, Triple<f64>>,
) -> bool {
    if !user_axes_location.contains_key(&axis_tag) {
        return false;
    }

    let axis_value_double = axis_value as f64;
    let axis_range = user_axes_location.get(&axis_tag).unwrap();
    axis_value_double < axis_range.minimum || axis_value_double > axis_range.maximum
}

impl<'a> SubsetTable<'a> for AxisValueFormat1<'a> {
    type ArgsForSubset = &'a [AxisRecord];
    type Output = bool;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut crate::serialize::Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let axis_index = self.axis_index();
        let axis_tag = args
            .get(axis_index as usize)
            .map(|x| x.axis_tag())
            .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        let axis_value = self.value().to_f32();

        if axis_value_is_outside_axis_range(axis_tag, axis_value, &plan.user_axes_location) {
            return Ok(false);
        }
        s.embed_bytes(self.min_table_bytes())?;
        Ok(true)
    }
}

impl<'a> SubsetTable<'a> for AxisValueFormat2<'a> {
    type ArgsForSubset = &'a [AxisRecord];
    type Output = bool;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut crate::serialize::Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let axis_index = self.axis_index();
        let axis_tag = args
            .get(axis_index as usize)
            .map(|x| x.axis_tag())
            .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        let axis_value = self.nominal_value().to_f32();

        if axis_value_is_outside_axis_range(axis_tag, axis_value, &plan.user_axes_location) {
            return Ok(false);
        }
        s.embed_bytes(self.min_table_bytes())?;
        Ok(true)
    }
}

impl<'a> SubsetTable<'a> for AxisValueFormat3<'a> {
    type ArgsForSubset = &'a [AxisRecord];
    type Output = bool;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut crate::serialize::Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let axis_index = self.axis_index();
        let axis_tag = args
            .get(axis_index as usize)
            .map(|x| x.axis_tag())
            .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        let axis_value = self.value().to_f32();

        if axis_value_is_outside_axis_range(axis_tag, axis_value, &plan.user_axes_location) {
            return Ok(false);
        }
        s.embed_bytes(self.min_table_bytes())?;
        Ok(true)
    }
}
impl<'a> SubsetTable<'a> for AxisValueFormat4<'a> {
    type ArgsForSubset = &'a [AxisRecord];
    type Output = bool;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut crate::serialize::Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let values = self.axis_values();
        for (i, axis_value) in values.iter().enumerate() {
            let axis_index = axis_value.axis_index();
            let axis_tag = args
                .get((axis_index + i as u16) as usize)
                .map(|x| x.axis_tag())
                .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
            let axis_value_f32 = axis_value.value().to_f32();
            if axis_value_is_outside_axis_range(axis_tag, axis_value_f32, &plan.user_axes_location)
            {
                return Ok(false);
            }
        }
        s.embed_bytes(self.min_table_bytes())?;
        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{types::NameId, FontRef, TableProvider};
    #[test]
    fn test_nameid_closure() {
        let mut plan = Plan::default();
        let font = FontRef::new(font_test_data::MATERIAL_SYMBOLS_SUBSET).unwrap();

        let stat = font.stat().unwrap();
        stat.collect_name_ids(&mut plan);
        assert_eq!(plan.name_ids.len(), 14);
        assert!(plan.name_ids.contains(NameId::new(2)));
        assert!(plan.name_ids.contains(NameId::new(256)));
        assert!(plan.name_ids.contains(NameId::new(257)));
        assert!(plan.name_ids.contains(NameId::new(258)));
        assert!(plan.name_ids.contains(NameId::new(259)));
        assert!(plan.name_ids.contains(NameId::new(267)));
        assert!(plan.name_ids.contains(NameId::new(268)));
        assert!(plan.name_ids.contains(NameId::new(260)));
        assert!(plan.name_ids.contains(NameId::new(261)));
        assert!(plan.name_ids.contains(NameId::new(262)));
        assert!(plan.name_ids.contains(NameId::new(263)));
        assert!(plan.name_ids.contains(NameId::new(264)));
        assert!(plan.name_ids.contains(NameId::new(265)));
        assert!(plan.name_ids.contains(NameId::new(266)));
    }
}
