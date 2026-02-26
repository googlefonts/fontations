//! impl subset() for fvar table

use crate::{
    serialize::SerializeErrorFlags, variations::solver::Triple, NameIdClosure, Plan, Subset,
    SubsetError,
};
use skrifa::raw::{tables::fvar::InstanceRecord, TopLevelTable};
use write_fonts::{read::tables::fvar::Fvar, types::Fixed};

impl NameIdClosure for Fvar<'_> {
    //TODO: support partial-instancing
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Ok(axis_instance_array) = self.axis_instance_arrays() else {
            return;
        };
        for axis in axis_instance_array.axes() {
            let tag = axis.axis_tag();
            if let Some(loc) = plan.user_axes_location.get(&tag) {
                if loc.is_point() {
                    continue;
                }
            }
            plan.name_ids.insert(axis.axis_name_id());
        }
        let old_axis_count = self.axis_count();
        for instance_record in axis_instance_array
            .instances()
            .iter()
            .filter_map(|x| x.ok())
        {
            if new_coords(&instance_record, plan, old_axis_count as usize).is_none()
                && !plan.axes_location.is_empty()
            {
                continue;
            }
            plan.name_ids.insert(instance_record.subfamily_name_id);
            if let Some(name_id) = instance_record.post_script_name_id {
                plan.name_ids.insert(name_id);
            }
        }
    }
}

impl Subset for Fvar<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &write_fonts::read::FontRef,
        s: &mut crate::serialize::Serializer,
        _builder: &mut write_fonts::FontBuilder,
    ) -> Result<(), crate::SubsetError> {
        if plan.axes_index_map.is_empty() {
            return Err(SubsetError::SubsetTableError(Fvar::TAG)); // empty
        }
        subset_fvar(self, plan, s).map_err(|_| SubsetError::SubsetTableError(Fvar::TAG))
    }
}

fn subset_fvar(
    fvar: &Fvar<'_>,
    plan: &Plan,
    s: &mut crate::serialize::Serializer,
) -> Result<(), SerializeErrorFlags> {
    let old_axis_count = fvar.axis_count();
    let new_axis_count = plan.axes_index_map.len() as u16;

    // Version
    s.embed(1_u16)?;
    s.embed(0_u16)?;
    let has_psname = fvar.instance_size() >= fvar.axis_count() * 4 + 6;
    s.embed(16_u16)?; // Axes array offset
    s.embed(2_u16)?; // reserved
    s.embed(new_axis_count)?;
    s.embed(20_u16)?; // axis size
    let instances = fvar
        .instances()
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
    let new_instance_coords = instances
        .iter()
        .flatten()
        .map(|i| new_coords(&i, plan, old_axis_count as usize))
        .collect::<Vec<_>>();
    s.embed(new_instance_coords.iter().filter(|x| x.is_some()).count() as u16)?;
    // Instance count
    s.embed(new_axis_count * 4 + if has_psname { 6 } else { 4 })?; // instance size
    for (ix, axis) in fvar
        .axes()
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        .iter()
        .enumerate()
    {
        if !plan.axes_index_map.contains_key(&ix) {
            continue;
        }
        s.embed(axis.axis_tag())?;
        if let Some(restricted_location) = plan.user_axes_location.get(&axis.axis_tag()) {
            s.embed(Fixed::from_f64(restricted_location.minimum))?;
            s.embed(Fixed::from_f64(restricted_location.middle))?;
            s.embed(Fixed::from_f64(restricted_location.maximum))?;
        } else {
            s.embed(axis.axis_tag())?;
            s.embed(axis.min_value())?;
            s.embed(axis.default_value())?;
            s.embed(axis.max_value())?;
        }
        s.embed(axis.flags())?;
        s.embed(axis.axis_name_id())?;
    }

    for (instance_record, new_coords) in instances
        .iter()
        .flatten()
        .zip(new_instance_coords.into_iter())
    {
        if let Some(coords) = new_coords {
            s.embed(instance_record.subfamily_name_id)?;
            s.embed(instance_record.flags)?;
            for coord in coords {
                s.embed(coord)?;
            }
            if let Some(name_id) = instance_record.post_script_name_id {
                s.embed(name_id)?;
            }
        }
    }

    Ok(())
}

fn new_coords(
    instance: &InstanceRecord<'_>,
    plan: &Plan,
    old_axis_count: usize,
) -> Option<Vec<Fixed>> {
    let coords = instance.coordinates;
    let axis_location = &plan.user_axes_location;
    let mut new_coords = vec![];
    if plan.axes_location.is_empty() {
        return None;
    }
    for axis in 0..old_axis_count {
        let tag = plan.axes_old_index_tag_map.get(&axis)?;
        let coord = coords[axis];
        if let Some(restricted_location) = axis_location.get(tag) {
            if !axis_coord_pinned_or_within_axis_range(
                coords.get(axis).map(|x| x.get()),
                restricted_location,
            ) {
                return None;
            }
            if restricted_location.is_point() {
                continue;
            }
        }
        new_coords.push(coord.get());
    }
    Some(new_coords)
}

fn axis_coord_pinned_or_within_axis_range(coord: Option<Fixed>, axis_limit: &Triple<f64>) -> bool {
    if let Some(coord) = coord {
        let axis_coord = coord.to_f32() as f64;
        if axis_limit.is_point() {
            if axis_limit.minimum != axis_coord {
                return false;
            }
        } else {
            if axis_coord < axis_limit.minimum || axis_coord > axis_limit.maximum {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{types::NameId, FontRef, TableProvider};
    #[test]
    fn test_nameid_closure() {
        let mut plan = Plan::default();
        let font = FontRef::new(font_test_data::MATERIAL_SYMBOLS_SUBSET).unwrap();

        let fvar = font.fvar().unwrap();
        fvar.collect_name_ids(&mut plan);
        assert_eq!(plan.name_ids.len(), 11);
        assert!(plan.name_ids.contains(NameId::new(256)));
        assert!(plan.name_ids.contains(NameId::new(257)));
        assert!(plan.name_ids.contains(NameId::new(258)));
        assert!(plan.name_ids.contains(NameId::new(259)));
        assert!(plan.name_ids.contains(NameId::new(260)));
        assert!(plan.name_ids.contains(NameId::new(261)));
        assert!(plan.name_ids.contains(NameId::new(262)));
        assert!(plan.name_ids.contains(NameId::new(263)));
        assert!(plan.name_ids.contains(NameId::new(264)));
        assert!(plan.name_ids.contains(NameId::new(265)));
        assert!(plan.name_ids.contains(NameId::new(266)));
    }
}
