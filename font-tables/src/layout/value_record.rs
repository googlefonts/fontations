//! A GPOS ValueRecord

use super::ValueFormat;
use crate::{
    parse_prelude::*,
    read::{ComputeSize, FontReadWithArgs, ReadArgs},
};

impl ValueFormat {
    /// Return the number of bytes required to store a [`ValueRecord`] in this format.
    #[inline]
    pub fn record_byte_len(self) -> usize {
        self.bits().count_ones() as usize * u16::RAW_BYTE_LEN
    }
}

/// A Positioning ValueRecord.
///
/// NOTE: we create these manually, since parsing is weird and depends on the
/// associated valueformat. That said, this isn't a great representation?
/// we could definitely do something much more in the zero-copy mode..
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ValueRecord {
    pub x_placement: Option<BigEndian<i16>>,
    pub y_placement: Option<BigEndian<i16>>,
    pub x_advance: Option<BigEndian<i16>>,
    pub y_advance: Option<BigEndian<i16>>,
    pub x_placement_device: Option<BigEndian<i16>>,
    pub y_placement_device: Option<BigEndian<i16>>,
    pub x_advance_device: Option<BigEndian<i16>>,
    pub y_advance_device: Option<BigEndian<i16>>,
}

impl ValueRecord {
    pub fn read_old(data: &[u8], format: ValueFormat) -> Result<Self, ReadError> {
        let data = FontData::new(data);
        Self::read(data, format)
    }

    pub fn read(data: FontData, format: ValueFormat) -> Result<Self, ReadError> {
        let mut this = ValueRecord::default();
        let mut cursor = data.cursor();

        if format.contains(ValueFormat::X_PLACEMENT) {
            this.x_placement = Some(cursor.read()?);
        }
        if format.contains(ValueFormat::Y_PLACEMENT) {
            this.y_placement = Some(cursor.read()?);
        }
        if format.contains(ValueFormat::X_ADVANCE) {
            this.x_advance = Some(cursor.read()?);
        }
        if format.contains(ValueFormat::Y_ADVANCE) {
            this.y_advance = Some(cursor.read()?);
        }
        if format.contains(ValueFormat::X_PLACEMENT_DEVICE) {
            this.x_placement_device = Some(cursor.read()?);
        }
        if format.contains(ValueFormat::Y_PLACEMENT_DEVICE) {
            this.y_placement_device = Some(cursor.read()?);
        }
        if format.contains(ValueFormat::X_ADVANCE_DEVICE) {
            this.x_advance_device = Some(cursor.read()?);
        }
        if format.contains(ValueFormat::Y_ADVANCE_DEVICE) {
            this.y_advance_device = Some(cursor.read()?);
        }
        Ok(this)
    }

    pub fn x_placement(&self) -> Option<i16> {
        self.x_placement.map(|val| val.get())
    }

    pub fn y_placement(&self) -> Option<i16> {
        self.y_placement.map(|val| val.get())
    }

    pub fn x_advance(&self) -> Option<i16> {
        self.x_advance.map(|val| val.get())
    }

    pub fn y_advance(&self) -> Option<i16> {
        self.y_advance.map(|val| val.get())
    }

    pub fn x_placement_device(&self) -> Option<i16> {
        self.x_placement_device.map(|val| val.get())
    }

    pub fn y_placement_device(&self) -> Option<i16> {
        self.y_placement_device.map(|val| val.get())
    }

    pub fn x_advance_device(&self) -> Option<i16> {
        self.x_advance_device.map(|val| val.get())
    }

    pub fn y_advance_device(&self) -> Option<i16> {
        self.y_advance_device.map(|val| val.get())
    }
}

impl ReadArgs for ValueRecord {
    type Args = ValueFormat;
}

impl<'a> FontReadWithArgs<'a> for ValueRecord {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        ValueRecord::read(data, *args)
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

impl ComputeSize for ValueRecord {
    #[inline]
    fn compute_size(args: &ValueFormat) -> usize {
        args.record_byte_len()
    }
}

impl<'a> SomeTable<'a> for ValueRecord {
    fn type_name(&self) -> &str {
        "ValueRecord"
    }

    // a total hack
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        let fields = [
            self.x_placement.is_some().then(|| "x_placement"),
            self.y_placement.is_some().then(|| "y_placement"),
            self.x_advance.is_some().then(|| "x_advance"),
            self.y_advance.is_some().then(|| "y_advance"),
            self.x_placement_device
                .is_some()
                .then(|| "x_placement_device"),
            self.y_placement_device
                .is_some()
                .then(|| "y_placement_device"),
            self.x_advance_device.is_some().then(|| "x_advance_device"),
            self.y_advance_device.is_some().then(|| "y_advance_device"),
        ];

        let name = fields.iter().filter_map(|x| *x).nth(idx)?;
        let typ: FieldType = match name {
            "x_placement" => self.x_placement().into(),
            "y_placement" => self.y_placement().into(),
            "x_advance" => self.x_advance().into(),
            "y_advance" => self.y_advance().into(),
            "x_placement_device" => self.x_placement_device().into(),
            "y_placement_device" => self.y_placement_device().into(),
            "x_advance_device" => self.x_advance_device().into(),
            "y_advance_device" => self.y_advance_device().into(),
            _ => panic!("hmm"),
        };

        Some(Field::new(name, typ))
    }
}
