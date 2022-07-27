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

#[derive(Clone, Default, PartialEq)]
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

// NOTE: this has a custom impl because it's a very funny case, being a record
// with variable length that is computed. Handling this in codegen doesn't
// feel totally worth it, to me
#[derive(Debug, Default, PartialEq)]
pub struct PairValueRecord {
    pub second_glyph: BigEndian<u16>,
    pub value_record1: ValueRecord,
    pub value_record2: ValueRecord,
}

impl ValueRecord {
    pub fn read_old(data: &[u8], format: ValueFormat) -> Result<Self, ReadError> {
        let data = FontData::new(data);
        Self::read(data, format)
    }

    pub fn read<'a>(data: FontData<'a>, format: ValueFormat) -> Result<Self, ReadError> {
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

impl ReadArgs for PairValueRecord {
    type Args = (ValueFormat, ValueFormat);
}

impl ComputeSize for PairValueRecord {
    #[inline]
    fn compute_size(args: &(ValueFormat, ValueFormat)) -> usize {
        args.0.record_byte_len() + args.1.record_byte_len() + u16::RAW_BYTE_LEN
    }
}

impl<'a> FontReadWithArgs<'a> for PairValueRecord {
    fn read_with_args(
        data: FontData<'a>,
        args: &(ValueFormat, ValueFormat),
    ) -> Result<PairValueRecord, ReadError> {
        let second_glyph = data.read_at(0)?;
        let range1 = 2..2 + args.0.record_byte_len();
        let range2 = range1.end..range1.end + args.1.record_byte_len();
        let value_record1 = data.read_with_args(range1, &args.0)?;
        let value_record2 = data.read_with_args(range2, &args.0)?;
        Ok(PairValueRecord {
            second_glyph,
            value_record1,
            value_record2,
        })
    }
}
