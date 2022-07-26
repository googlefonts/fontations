/// A GPOS ValueRecord
use super::ValueFormat;
use crate::parse_prelude::*;

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

impl ValueRecord {
    pub fn read(data: &[u8], format: ValueFormat) -> Result<Self, ReadError> {
        let mut this = ValueRecord::default();
        let mut words = data
            .chunks_exact(2)
            .map(|bytes| BigEndian::<i16>::new(bytes.try_into().unwrap()));

        if format.contains(ValueFormat::X_PLACEMENT) {
            this.x_placement = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        if format.contains(ValueFormat::Y_PLACEMENT) {
            this.y_placement = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        if format.contains(ValueFormat::X_ADVANCE) {
            this.x_advance = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        if format.contains(ValueFormat::Y_ADVANCE) {
            this.y_advance = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        if format.contains(ValueFormat::X_PLACEMENT_DEVICE) {
            this.x_placement_device = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        if format.contains(ValueFormat::Y_PLACEMENT_DEVICE) {
            this.y_placement_device = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        if format.contains(ValueFormat::X_ADVANCE_DEVICE) {
            this.x_advance_device = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        if format.contains(ValueFormat::Y_ADVANCE_DEVICE) {
            this.y_advance_device = Some(words.next().ok_or(ReadError::OutOfBounds)?);
        }
        Ok(this)
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
