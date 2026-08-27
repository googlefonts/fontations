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
    let mut effective = value_record.format();

    if strip_hints {
        effective -= ValueFormat::ANY_DEVICE_OR_VARIDX;
    }

    if !strip_empty {
        return effective;
    }

    // A field contributes nothing when its sixteen bits are zero, whether it
    // holds a value or an offset to a device table. The two cases are treated
    // alike: `strip_empty` governs both.
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

    if effective.intersects(ValueFormat::ANY_DEVICE_OR_VARIDX) {
        for field in DEVICE_FIELDS {
            if value_record
                .device_offset(field)
                .is_some_and(|offset| offset.is_null())
            {
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

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::FontData;

    const ALL: ValueFormat = ValueFormat::from_bits_truncate(0x00FF);

    /// A record in `ALL` format: four values then four device offsets.
    fn record(bytes: &[u8]) -> ValueRecord<'_> {
        ValueRecord::new(FontData::new(bytes), 0, ALL)
    }

    /// Bytes for a record whose every field is zero.
    const EMPTY: [u8; 16] = [0; 16];

    /// x_placement = 1, x_placement_device -> 12; everything else zero.
    const PARTLY_SET: [u8; 16] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0];

    #[test]
    fn keeps_everything_when_stripping_nothing() {
        // with neither flag set the format is reported as-is, even for fields
        // that are zero: the caller has not asked for anything to be dropped
        assert_eq!(compute_effective_format(&record(&EMPTY), false, false), ALL);
        assert_eq!(
            compute_effective_format(&record(&PARTLY_SET), false, false),
            ALL
        );
    }

    #[test]
    fn strip_empty_drops_zero_values_and_null_offsets() {
        // an all-zero record keeps nothing
        assert_eq!(
            compute_effective_format(&record(&EMPTY), false, true),
            ValueFormat::empty()
        );
        // and only the two fields that are actually set survive
        assert_eq!(
            compute_effective_format(&record(&PARTLY_SET), false, true),
            ValueFormat::X_PLACEMENT | ValueFormat::X_PLACEMENT_DEVICE
        );
    }

    #[test]
    fn strip_hints_drops_devices_however_they_are_set() {
        // device fields go whether or not their offsets are null, and whether
        // or not empty fields are being stripped
        assert_eq!(
            compute_effective_format(&record(&PARTLY_SET), true, false),
            ALL - ValueFormat::ANY_DEVICE_OR_VARIDX
        );
        assert_eq!(
            compute_effective_format(&record(&PARTLY_SET), true, true),
            ValueFormat::X_PLACEMENT
        );
    }

    /// Fields absent from the format are never reported present, whatever the
    /// bytes underneath happen to say.
    #[test]
    fn absent_fields_stay_absent() {
        let format = ValueFormat::Y_ADVANCE | ValueFormat::Y_ADVANCE_DEVICE;
        let bytes = [0u8, 7, 0, 12];
        let rec = ValueRecord::new(FontData::new(&bytes), 0, format);
        assert_eq!(compute_effective_format(&rec, false, true), format);
        assert_eq!(
            compute_effective_format(&rec, true, true),
            ValueFormat::Y_ADVANCE
        );
    }
}
