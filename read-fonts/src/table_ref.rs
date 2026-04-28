use std::ops::Range;

use crate::types::FixedSize;
use crate::FontData;

// https://github.com/harfbuzz/harfbuzz/blob/aba63bb5/src/hb-null.hh#L40
/// The number of bytes required to represent the largest table we have.
///
/// This is checked by an assert at compile time, and can be increased as needed.
pub const NULL_POOL_SIZE: usize = 262;

const ARR_LEN: usize = NULL_POOL_SIZE + u16::RAW_BYTE_LEN;

/// This is [0, 1] ('1' in u16be) followed by NULL_POOL_SIZE zeros.
///
/// - this same array is reused both for format-1 tables (which need a leading 1)
///   as well as all other tables, which don't.
static EMPTY_TABLE_BYTES: [u8; ARR_LEN] = {
    let mut arr = [0u8; ARR_LEN];
    arr[1] = 1;
    arr
};

impl FontData<'static> {
    /// Return all zeroes suitable for the default impl of a table.
    pub(crate) fn default_table_data() -> Self {
        FontData::new(&EMPTY_TABLE_BYTES[2..])
    }

    /// Return a [0x0, 0x01] byte pair (u16be) and then all zeros, to represent
    /// the default impl of a format 1 table with u16 format.
    pub(crate) fn default_format_1_u16_table_data() -> Self {
        FontData::new(&EMPTY_TABLE_BYTES)
    }

    /// Return a single 0x01 and then all zeros, to represent the default impl
    /// of a format 1 table with u8 format.
    pub(crate) fn default_format_1_u8_table_data() -> Self {
        FontData::new(&EMPTY_TABLE_BYTES[1..])
    }
}

/// Return the minimum range of the table bytes
///
/// This trait is implemented in generated code, and we use this to get the
/// minimum length/bytes of a table.
pub trait MinByteRange<'a> {
    fn min_byte_range(&self) -> Range<usize>;
    fn min_table_bytes(&self) -> &'a [u8];
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn how_does_big_endian_work_again() {
        let data = FontData::default_format_1_u16_table_data();
        assert_eq!(data.read_at(0), Ok(1u16));

        assert_eq!(
            FontData::default_format_1_u8_table_data().read_at(0),
            Ok(1u8)
        );
    }
}
