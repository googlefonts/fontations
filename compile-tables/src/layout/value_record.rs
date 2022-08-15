//! The ValueRecord type used in the GPOS table

#[cfg(feature = "parsing")]
use crate::compile_prelude::{FontData, FromObjRef};

use super::gpos::ValueFormat;
use crate::{
    validate::Validate,
    write::{FontWrite, TableWriter},
};

#[derive(Clone, Default, PartialEq)]
pub struct ValueRecord {
    pub x_placement: Option<i16>,
    pub y_placement: Option<i16>,
    pub x_advance: Option<i16>,
    pub y_advance: Option<i16>,
    pub x_placement_device: Option<i16>,
    pub y_placement_device: Option<i16>,
    pub x_advance_device: Option<i16>,
    pub y_advance_device: Option<i16>,
}

impl ValueRecord {
    /// The [ValueFormat] of this record.
    pub fn format(&self) -> ValueFormat {
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
}

impl FontWrite for ValueRecord {
    fn write_into(&self, writer: &mut TableWriter) {
        macro_rules! write_field {
            ($field:expr) => {
                if let Some(v) = $field {
                    v.write_into(writer);
                }
            };
        }

        write_field!(self.x_placement);
        write_field!(self.y_placement);
        write_field!(self.x_advance);
        write_field!(self.y_advance);
        write_field!(self.x_placement_device);
        write_field!(self.y_placement_device);
        write_field!(self.x_advance_device);
        write_field!(self.y_advance_device);
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
            .map(|x| f.field("x_placement_device", &x));
        self.y_placement_device
            .map(|y| f.field("y_placement_device", &y));
        self.x_advance_device
            .map(|x| f.field("x_advance_device", &x));
        self.y_advance_device
            .map(|y| f.field("y_advance_device", &y));
        f.finish()
    }
}

impl Validate for ValueRecord {
    fn validate_impl(&self, _ctx: &mut crate::validate::ValidationCtx) {}
}

#[cfg(feature = "parsing")]
impl FromObjRef<font_tables::layout::gpos::ValueRecord> for ValueRecord {
    fn from_obj_ref(from: &font_tables::layout::gpos::ValueRecord, _data: &FontData) -> Self {
        ValueRecord {
            x_placement: from.x_placement.map(|val| val.get()),
            y_placement: from.y_placement.map(|val| val.get()),
            x_advance: from.x_advance.map(|val| val.get()),
            y_advance: from.y_advance.map(|val| val.get()),
            x_placement_device: from.x_placement_device.map(|val| val.get()),
            y_placement_device: from.y_placement_device.map(|val| val.get()),
            x_advance_device: from.x_advance_device.map(|val| val.get()),
            y_advance_device: from.y_advance_device.map(|val| val.get()),
        }
    }
}
