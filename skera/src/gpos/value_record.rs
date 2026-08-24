//! impl subset() for ValueRecord

use crate::{
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, SerializeResultEmpty, Serializer},
    CollectVariationIndices, Plan, SubsetTable,
};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::gpos::{ValueFormat, ValueRecord},
    },
    types::Offset16,
};

/// The device fields of a value record, in on-disk order.
const DEVICE_FIELDS: [ValueFormat; 4] = [
    ValueFormat::X_PLACEMENT_DEVICE,
    ValueFormat::Y_PLACEMENT_DEVICE,
    ValueFormat::X_ADVANCE_DEVICE,
    ValueFormat::Y_ADVANCE_DEVICE,
];

pub(crate) fn compute_effective_format(
    value_record: &ValueRecord,
    strip_hints: bool,
    strip_empty: bool,
) -> ValueFormat {
    let format = value_record.format();
    let mut effective = format;

    if strip_hints {
        effective -= ValueFormat::ANY_DEVICE_OR_VARIDX;
    } else if format.intersects(ValueFormat::ANY_DEVICE_OR_VARIDX) {
        // a device field that is present but null contributes nothing
        for field in DEVICE_FIELDS {
            if value_record
                .device_offset(field)
                .is_some_and(|offset| offset.is_null())
            {
                effective -= field;
            }
        }
    }

    if strip_empty {
        for (field, value) in [
            (ValueFormat::X_PLACEMENT, value_record.x_placement()),
            (ValueFormat::Y_PLACEMENT, value_record.y_placement()),
            (ValueFormat::X_ADVANCE, value_record.x_advance()),
            (ValueFormat::Y_ADVANCE, value_record.y_advance()),
        ] {
            if value == Some(0) {
                effective -= field;
            }
        }
    }

    effective
}

impl<'a> SubsetTable<'a> for ValueRecord<'_> {
    type ArgsForSubset = ValueFormat;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        new_format: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        if new_format.is_empty() {
            return Ok(());
        }

        if new_format.contains(ValueFormat::X_PLACEMENT) {
            s.embed(self.x_placement().unwrap_or(0))?;
        }

        if new_format.contains(ValueFormat::Y_PLACEMENT) {
            s.embed(self.y_placement().unwrap_or(0))?;
        }

        if new_format.contains(ValueFormat::X_ADVANCE) {
            s.embed(self.x_advance().unwrap_or(0))?;
        }

        if new_format.contains(ValueFormat::Y_ADVANCE) {
            s.embed(self.y_advance().unwrap_or(0))?;
        }

        if !new_format.intersects(ValueFormat::ANY_DEVICE_OR_VARIDX) {
            return Ok(());
        }

        for (field, device) in [
            (ValueFormat::X_PLACEMENT_DEVICE, self.x_placement_device()),
            (ValueFormat::Y_PLACEMENT_DEVICE, self.y_placement_device()),
            (ValueFormat::X_ADVANCE_DEVICE, self.x_advance_device()),
            (ValueFormat::Y_ADVANCE_DEVICE, self.y_advance_device()),
        ] {
            if !new_format.contains(field) {
                continue;
            }
            let offset_pos = s.embed(0_u16)?;
            if let Some(device) = device
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                Offset16::serialize_subset(
                    &device,
                    s,
                    plan,
                    &plan.layout_varidx_delta_map,
                    offset_pos,
                )
                .is_empty()?;
            }
        }
        Ok(())
    }
}

impl CollectVariationIndices for ValueRecord<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if !self.format().intersects(ValueFormat::ANY_DEVICE_OR_VARIDX) {
            return;
        }

        for device in [
            self.x_placement_device(),
            self.y_placement_device(),
            self.x_advance_device(),
            self.y_advance_device(),
        ]
        .into_iter()
        .flatten()
        .flatten()
        {
            device.collect_variation_indices(plan, varidx_set);
        }
    }
}
