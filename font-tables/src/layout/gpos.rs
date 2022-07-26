//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

#[path = "./valuerecord.rs"]
mod valuerecord;

use super::{ClassDef, CoverageTable, Device, FeatureList, FeatureVariations, ScriptList};
pub use valuerecord::ValueRecord;

include!("../../generated/gpos.rs");

impl ValueFormat {
    /// Return the number of bytes required to store a [`ValueRecord`] in this format.
    #[inline]
    pub fn record_byte_len(self) -> usize {
        self.bits().count_ones() as usize * u16::RAW_BYTE_LEN
    }
}

fn class1_record_len(
    class1_count: u16,
    class2_count: u16,
    format1: ValueFormat,
    format2: ValueFormat,
) -> usize {
    (format1.record_byte_len() + format2.record_byte_len())
        * class1_count as usize
        * class2_count as usize
}

impl<'a> SinglePosFormat1<'a> {
    pub fn value_record(&self) -> ValueRecord {
        self.data
            .read_at_with(self.shape.value_record_byte_range().start, |bytes| {
                ValueRecord::read(bytes, self.value_format())
            })
            .unwrap_or_default()
    }
}

impl<'a> SinglePosFormat2<'a> {
    pub fn value_records(&self) -> impl Iterator<Item = ValueRecord> + '_ {
        let count = self.value_count() as usize;
        let format = self.value_format();

        (0..count).map(move |idx| {
            let offset =
                self.shape.value_records_byte_range().start + (idx * format.record_byte_len());
            self.data
                .read_at_with(offset, |bytes| ValueRecord::read(bytes, format))
                .unwrap_or_default()
        })
    }
}
