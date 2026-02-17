//! impl subset() for ValueRecord

use crate::{
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, Serializer},
    CollectVariationIndices, Plan, SubsetTable,
};
use skrifa::raw::tables::gpos::DeviceOrVariationIndex;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::gpos::{ValueFormat, ValueRecord},
        FontData,
    },
    types::Offset16,
};

pub(crate) fn compute_effective_format(
    value_record: &ValueRecord,
    strip_hints: bool,
    strip_empty: bool,
    font_data: FontData,
    plan: Option<&Plan>,
) -> ValueFormat {
    log::info!(
        "Computing effective format for ValueRecord: {:?}, strip_hints={}, strip_empty={}",
        value_record,
        strip_hints,
        strip_empty
    );
    let mut value_format = ValueFormat::empty();

    if let Some(x_placement) = value_record.x_placement {
        if !strip_empty || x_placement.get() != 0 {
            value_format |= ValueFormat::X_PLACEMENT;
        }
    }

    if let Some(y_placement) = value_record.y_placement {
        if !strip_empty || y_placement.get() != 0 {
            value_format |= ValueFormat::Y_PLACEMENT;
        }
    }

    if let Some(x_advance) = value_record.x_advance {
        if !strip_empty || x_advance.get() != 0 {
            value_format |= ValueFormat::X_ADVANCE;
        }
    }

    if let Some(y_advance) = value_record.y_advance {
        if !strip_empty || y_advance.get() != 0 {
            value_format |= ValueFormat::Y_ADVANCE;
        }
    }

    if !value_record.x_placement_device.get().is_null() && !strip_hints {
        // During instancing, don't include device format flags - deltas are already applied to base values
        if let Some(plan_ref) = plan {
            if !plan_ref.normalized_coords.is_empty() {
                log::info!("X_PLACEMENT_DEVICE: not adding format bit during instancing");
                // Skip device format during instancing
            } else {
                value_format |= ValueFormat::X_PLACEMENT_DEVICE;
                log::info!("X_PLACEMENT_DEVICE: keeping flag (not instancing)");
            }
        } else {
            value_format |= ValueFormat::X_PLACEMENT_DEVICE;
            log::info!("X_PLACEMENT_DEVICE: keeping flag (no plan)");
        }
    }

    if !value_record.y_placement_device.get().is_null() && !strip_hints {
        if let Some(plan_ref) = plan {
            if !plan_ref.normalized_coords.is_empty() {
                log::info!("Y_PLACEMENT_DEVICE: not adding format bit during instancing");
            } else {
                value_format |= ValueFormat::Y_PLACEMENT_DEVICE;
                log::info!("Y_PLACEMENT_DEVICE: keeping flag (not instancing)");
            }
        } else {
            value_format |= ValueFormat::Y_PLACEMENT_DEVICE;
        }
    }

    if !value_record.x_advance_device.get().is_null() && !strip_hints {
        if let Some(plan_ref) = plan {
            if !plan_ref.normalized_coords.is_empty() {
                log::info!("X_ADVANCE_DEVICE: not adding format bit during instancing");
            } else {
                value_format |= ValueFormat::X_ADVANCE_DEVICE;
                log::info!("X_ADVANCE_DEVICE: keeping flag (not instancing)");
            }
        } else {
            value_format |= ValueFormat::X_ADVANCE_DEVICE;
        }
    }

    if !value_record.y_advance_device.get().is_null() && !strip_hints {
        if let Some(plan_ref) = plan {
            if !plan_ref.normalized_coords.is_empty() {
                log::info!("Y_ADVANCE_DEVICE: not adding format bit during instancing");
            } else {
                value_format |= ValueFormat::Y_ADVANCE_DEVICE;
                log::info!("Y_ADVANCE_DEVICE: keeping flag (not instancing)");
            }
        } else {
            value_format |= ValueFormat::Y_ADVANCE_DEVICE;
        }
    }
    value_format
}

/// Apply delta to a base value if applicable during instancing.
/// For now, we don't apply deltas at the base value level as the device/varidx handling
/// is done through the Device subset logic. This is a placeholder for future enhancements.
fn apply_value_delta(
    value_record: &ValueRecord,
    which_one: ValueFormat,
    font_data: FontData,
    plan: &Plan,
) -> i16 {
    let base = match which_one {
        ValueFormat::X_PLACEMENT => value_record.x_placement.unwrap_or_default().get(),
        ValueFormat::Y_PLACEMENT => value_record.y_placement.unwrap_or_default().get(),
        ValueFormat::X_ADVANCE => value_record.x_advance.unwrap_or_default().get(),
        ValueFormat::Y_ADVANCE => value_record.y_advance.unwrap_or_default().get(),
        _ => 0, // For device/varidx fields, the deltas are handled in the device subset logic}
    };
    let device_offset = match which_one {
        ValueFormat::X_PLACEMENT => value_record
            .x_placement_device(font_data)
            .transpose()
            .ok()
            .flatten(),
        ValueFormat::Y_PLACEMENT => value_record
            .y_placement_device(font_data)
            .transpose()
            .ok()
            .flatten(),
        ValueFormat::X_ADVANCE => value_record
            .x_advance_device(font_data)
            .transpose()
            .ok()
            .flatten(),
        ValueFormat::Y_ADVANCE => value_record
            .y_advance_device(font_data)
            .transpose()
            .ok()
            .flatten(),
        _ => None,
    };
    if let Some(DeviceOrVariationIndex::VariationIndex(varidx)) = device_offset {
        // Encode the two-level variation index as a single u32:
        // combine outer and inner indices as (outer << 16) | inner
        let combined_idx = ((varidx.delta_set_outer_index() as u32) << 16)
            | (varidx.delta_set_inner_index() as u32);
        if let Some((_idx, delta)) = plan.layout_varidx_delta_map.borrow().get(&combined_idx) {
            log::info!(
                "Applying value delta for {:?} with record: {:?}, delta {}",
                which_one,
                value_record,
                delta
            );
            return base.saturating_add(*delta as i16);
        }
    }
    base
}

impl<'a> SubsetTable<'a> for ValueRecord {
    type ArgsForSubset = (ValueFormat, FontData<'a>);
    type Output = ();

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (new_format, font_data) = args;
        if new_format.is_empty() {
            return Ok(());
        }

        if new_format.contains(ValueFormat::X_PLACEMENT) {
            let value = apply_value_delta(self, ValueFormat::X_PLACEMENT, font_data, _plan);
            s.embed(value)?;
        }

        if new_format.contains(ValueFormat::Y_PLACEMENT) {
            let value = apply_value_delta(self, ValueFormat::Y_PLACEMENT, font_data, _plan);
            s.embed(value)?;
        }

        if new_format.contains(ValueFormat::X_ADVANCE) {
            let value = apply_value_delta(self, ValueFormat::X_ADVANCE, font_data, _plan);
            s.embed(value)?;
        }

        if new_format.contains(ValueFormat::Y_ADVANCE) {
            let value = apply_value_delta(self, ValueFormat::Y_ADVANCE, font_data, _plan);
            s.embed(value)?;
        }

        if !new_format.intersects(ValueFormat::ANY_DEVICE_OR_VARIDX) {
            return Ok(());
        }

        if new_format.contains(ValueFormat::X_PLACEMENT_DEVICE) {
            if let Some(device) = self
                .x_placement_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                let offset_pos = s.embed(0_u16)?;
                Offset16::serialize_subset(
                    &device,
                    s,
                    _plan,
                    &_plan.layout_varidx_delta_map.borrow(),
                    offset_pos,
                )?;
            }
        }

        if new_format.contains(ValueFormat::Y_PLACEMENT_DEVICE) {
            if let Some(device) = self
                .y_placement_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                let offset_pos = s.embed(0_u16)?;
                Offset16::serialize_subset(
                    &device,
                    s,
                    _plan,
                    &_plan.layout_varidx_delta_map.borrow(),
                    offset_pos,
                )?;
            }
        }

        if new_format.contains(ValueFormat::X_ADVANCE_DEVICE) {
            if let Some(device) = self
                .x_advance_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                let offset_pos = s.embed(0_u16)?;
                Offset16::serialize_subset(
                    &device,
                    s,
                    _plan,
                    &_plan.layout_varidx_delta_map.borrow(),
                    offset_pos,
                )?;
            }
        }

        if new_format.contains(ValueFormat::Y_ADVANCE_DEVICE) {
            if let Some(device) = self
                .y_advance_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                let offset_pos = s.embed(0_u16)?;
                Offset16::serialize_subset(
                    &device,
                    s,
                    _plan,
                    &_plan.layout_varidx_delta_map.borrow(),
                    offset_pos,
                )?;
            }
        }

        Ok(())
    }
}

pub(crate) fn collect_variation_indices(
    value_record: &ValueRecord,
    font_data: FontData,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
) {
    let value_format = value_record.format;
    if !value_format.intersects(ValueFormat::ANY_DEVICE_OR_VARIDX) {
        return;
    }

    if let Some(Ok(x_pla_device)) = value_record.x_placement_device(font_data) {
        x_pla_device.collect_variation_indices(plan, varidx_set);
    }

    if let Some(Ok(y_pla_device)) = value_record.y_placement_device(font_data) {
        y_pla_device.collect_variation_indices(plan, varidx_set);
    }

    if let Some(Ok(x_adv_device)) = value_record.x_advance_device(font_data) {
        x_adv_device.collect_variation_indices(plan, varidx_set);
    }

    if let Some(Ok(y_adv_device)) = value_record.y_advance_device(font_data) {
        y_adv_device.collect_variation_indices(plan, varidx_set);
    }
}
