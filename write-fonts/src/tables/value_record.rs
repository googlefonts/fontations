//! The ValueRecord type used in the GPOS table

use read_fonts::FontData;

use super::ValueFormat;
use crate::{
    from_obj::{FromObjRef, ToOwnedObj},
    offsets::NullableOffsetMarker,
    tables::layout::DeviceOrVariationIndex,
    validate::Validate,
    write::{FontWrite, TableWriter},
};

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ValueRecord {
    // Okay so... this demands some explanation.
    //
    // In general, we compute the format for a value record by looking at what
    // fields are present in the record. This works 99% of the time.
    //
    // The problem, though, is that in certain cases we need to create empty
    // value records that have an explicit format. In particular this occurs
    // in class-based pairpos tables, where it is possible that two classes
    // have no relationship, but we still need to put a record of the
    // appropriate size in the array of value records.
    //
    // In this case, we cannot infer the correct format, because when
    // we see any null offsets we will assume that those fields should not
    // be present in the record, where in fact we want to have explicit
    // null offsets.
    //
    // To handle this, we allow the user to pass an explicit format when
    // constructing a value record. If this field is present, we will use it
    // instead of computing the format.
    explicit_format: Option<ValueFormat>,
    pub x_placement: Option<i16>,
    pub y_placement: Option<i16>,
    pub x_advance: Option<i16>,
    pub y_advance: Option<i16>,
    pub x_placement_device: NullableOffsetMarker<DeviceOrVariationIndex>,
    pub y_placement_device: NullableOffsetMarker<DeviceOrVariationIndex>,
    pub x_advance_device: NullableOffsetMarker<DeviceOrVariationIndex>,
    pub y_advance_device: NullableOffsetMarker<DeviceOrVariationIndex>,
}

impl ValueRecord {
    pub fn new() -> ValueRecord {
        ValueRecord::default()
    }

    pub fn with_x_placement(mut self, val: i16) -> Self {
        self.x_placement = Some(val);
        self
    }

    pub fn with_y_placement(mut self, val: i16) -> Self {
        self.y_placement = Some(val);
        self
    }

    pub fn with_x_advance(mut self, val: i16) -> Self {
        self.x_advance = Some(val);
        self
    }

    pub fn with_y_advance(mut self, val: i16) -> Self {
        self.y_advance = Some(val);
        self
    }

    pub fn with_x_placement_device(mut self, val: impl Into<DeviceOrVariationIndex>) -> Self {
        self.x_placement_device = val.into().into();
        self
    }

    pub fn with_y_placement_device(mut self, val: impl Into<DeviceOrVariationIndex>) -> Self {
        self.y_placement_device = val.into().into();
        self
    }

    pub fn with_x_advance_device(mut self, val: impl Into<DeviceOrVariationIndex>) -> Self {
        self.x_advance_device = val.into().into();
        self
    }

    pub fn with_y_advance_device(mut self, val: impl Into<DeviceOrVariationIndex>) -> Self {
        self.y_advance_device = val.into().into();
        self
    }

    pub fn with_explicit_value_format(mut self, format: ValueFormat) -> Self {
        self.set_explicit_value_format(format);
        self
    }

    /// Set an explicit ValueFormat, overriding the computed format.
    ///
    /// Use this method if you wish to write a ValueFormat that includes
    /// explicit null offsets for any of the device or variation index tables.
    pub fn set_explicit_value_format(&mut self, format: ValueFormat) {
        self.explicit_format = Some(format)
    }

    /// The [ValueFormat] of this record.
    pub fn format(&self) -> ValueFormat {
        if let Some(format) = self.explicit_format {
            return format;
        }

        macro_rules! flag_if_true {
            ($field:expr, $flag:expr) => {
                $field
                    .is_some()
                    .then(|| $flag)
                    .unwrap_or(ValueFormat::empty())
            };
        }

        flag_if_true!(self.x_placement, ValueFormat::X_PLACEMENT)
            | flag_if_true!(self.y_placement, ValueFormat::Y_PLACEMENT)
            | flag_if_true!(self.x_advance, ValueFormat::X_ADVANCE)
            | flag_if_true!(self.y_advance, ValueFormat::Y_ADVANCE)
            | flag_if_true!(self.x_placement_device, ValueFormat::X_PLACEMENT_DEVICE)
            | flag_if_true!(self.y_placement_device, ValueFormat::Y_PLACEMENT_DEVICE)
            | flag_if_true!(self.x_advance_device, ValueFormat::X_ADVANCE_DEVICE)
            | flag_if_true!(self.y_advance_device, ValueFormat::Y_ADVANCE_DEVICE)
    }

    /// Return the number of bytes required to encode this value record
    pub fn encoded_size(&self) -> usize {
        self.format().bits().count_ones() as usize * 2
    }
}

impl FontWrite for ValueRecord {
    fn write_into(&self, writer: &mut TableWriter) {
        let format = self.format();
        macro_rules! write_field {
            ($field:expr, $flag:expr) => {
                if format.contains($flag) {
                    $field.unwrap_or_default().write_into(writer);
                }
            };
            ($field:expr, $flag:expr, off) => {
                if format.contains($flag) {
                    $field.write_into(writer);
                }
            };
        }

        write_field!(self.x_placement, ValueFormat::X_PLACEMENT);
        write_field!(self.y_placement, ValueFormat::Y_PLACEMENT);
        write_field!(self.x_advance, ValueFormat::X_ADVANCE);
        write_field!(self.y_advance, ValueFormat::Y_ADVANCE);
        write_field!(
            self.x_placement_device,
            ValueFormat::X_PLACEMENT_DEVICE,
            off
        );
        write_field!(
            self.y_placement_device,
            ValueFormat::Y_PLACEMENT_DEVICE,
            off
        );
        write_field!(self.x_advance_device, ValueFormat::X_ADVANCE_DEVICE, off);
        write_field!(self.y_advance_device, ValueFormat::Y_ADVANCE_DEVICE, off);
    }
}

impl std::fmt::Debug for ValueRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut f = f.debug_struct("ValueRecord");
        self.x_placement.map(|x| f.field("x_placement", &x));
        self.y_placement.map(|y| f.field("y_placement", &y));
        self.x_advance.map(|x| f.field("x_advance", &x));
        self.y_advance.map(|y| f.field("y_advance", &y));
        self.x_placement_device
            .as_ref()
            .map(|x| f.field("x_placement_device", &x));
        self.y_placement_device
            .as_ref()
            .map(|y| f.field("y_placement_device", &y));
        self.x_advance_device
            .as_ref()
            .map(|x| f.field("x_advance_device", &x));
        self.y_advance_device
            .as_ref()
            .map(|y| f.field("y_advance_device", &y));
        f.finish()
    }
}

impl Validate for ValueRecord {
    fn validate_impl(&self, _ctx: &mut crate::validate::ValidationCtx) {}
}

impl FromObjRef<read_fonts::tables::gpos::ValueRecord> for ValueRecord {
    fn from_obj_ref(from: &read_fonts::tables::gpos::ValueRecord, data: FontData) -> Self {
        ValueRecord {
            explicit_format: None,
            x_placement: from.x_placement(),
            y_placement: from.y_placement(),
            x_advance: from.x_advance(),
            y_advance: from.y_advance(),
            x_placement_device: from.x_placement_device(data).to_owned_obj(data),
            y_placement_device: from.y_placement_device(data).to_owned_obj(data),
            x_advance_device: from.x_advance_device(data).to_owned_obj(data),
            y_advance_device: from.y_advance_device(data).to_owned_obj(data),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serialize_explicit_value_record() {
        let mut my_record = ValueRecord {
            x_advance: Some(5),
            ..Default::default()
        };
        my_record.set_explicit_value_format(ValueFormat::X_ADVANCE | ValueFormat::X_ADVANCE_DEVICE);
        let bytes = crate::dump_table(&my_record).unwrap();
        assert_eq!(bytes.len(), 4);
        let read_back =
            read_fonts::tables::gpos::ValueRecord::read(FontData::new(&bytes), my_record.format())
                .unwrap();
        assert!(read_back.x_advance_device.get().is_null());
    }
}
