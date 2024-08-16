//! impl subset() for OS/2
use crate::SubsetFlags;
use crate::{Plan, SubsetError, SubsetError::SubsetTableError};
use std::cmp::Ordering;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::os2::{Os2, OS2_UNICODE_RANGES},
        FontRef, TableProvider, TopLevelTable,
    },
    FontBuilder,
};

// reference: subset() for OS/2 in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-ot-os2-table.hh#L229
pub(crate) fn subset_os2(
    font: &FontRef,
    plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let os2 = font.os2().or(Err(SubsetTableError(Os2::TAG)))?;
    let mut out = os2.offset_data().as_bytes().to_owned();

    let us_first_char_index: u16 = plan.unicodes.first().unwrap_or(0xFFFF).min(0xFFFF) as u16;
    out.get_mut(64..66)
        .unwrap()
        .copy_from_slice(&us_first_char_index.to_be_bytes());

    let us_last_char_index: u16 = plan.unicodes.last().unwrap_or(0xFFFF).min(0xFFFF) as u16;
    out.get_mut(66..68)
        .unwrap()
        .copy_from_slice(&us_last_char_index.to_be_bytes());

    if !plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES)
    {
        update_unicode_ranges(&plan.unicodes, out.get_mut(42..58).unwrap());
    }

    builder.add_raw(Os2::TAG, out);
    Ok(())
}

fn update_unicode_ranges(unicodes: &IntSet<u32>, ul_unicode_range: &mut [u8]) {
    let mut new_ranges = [0_u32; 4];

    for cp in unicodes.iter() {
        let Some(bit) = get_unicode_range_bit(cp) else {
            continue;
        };
        if bit < 128 {
            let block = (bit / 32) as usize;
            let bit_in_block = bit % 32;
            let mask = 1 << bit_in_block;
            new_ranges[block] |= mask;
        }

        // the spec says that bit 57 ("Non Plane 0") implies that there's
        // at least one codepoint beyond the BMP; so I also include all
        // the non-BMP codepoints here
        if (0x10000..=0x110000).contains(&cp) {
            new_ranges[1] |= 1 << 25;
        }
    }

    for (i, cp) in new_ranges.iter().enumerate() {
        let new_range = cp.to_be_bytes();
        let org_range = ul_unicode_range.get_mut(i * 4..i * 4 + 4).unwrap();
        //set bits only if set in the original
        for idx in 0..4 {
            org_range[idx] &= new_range[idx];
        }
    }
}

// Returns the bit to be set in os/2 ulUnicodeOS2Range for a given codepoint.
fn get_unicode_range_bit(cp: u32) -> Option<u8> {
    OS2_UNICODE_RANGES
        .binary_search_by(|&(a, b, _)| OS2Range::new(a, b).cmp(cp))
        .ok()
        .map(|i| OS2_UNICODE_RANGES[i].2)
}

pub struct OS2Range {
    first: u32,
    last: u32,
}

impl OS2Range {
    pub const fn new(a: u32, b: u32) -> Self {
        Self { first: a, last: b }
    }

    fn cmp(&self, key: u32) -> Ordering {
        if key < self.first {
            Ordering::Greater
        } else if key <= self.last {
            Ordering::Equal
        } else {
            Ordering::Less
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_get_unicode_range_bit() {
        assert_eq!(get_unicode_range_bit(0x0), Some(0));
        assert_eq!(get_unicode_range_bit(0x0042), Some(0));
        assert_eq!(get_unicode_range_bit(0x007F), Some(0));
        assert_eq!(get_unicode_range_bit(0x0080), Some(1));

        assert_eq!(get_unicode_range_bit(0x30A0), Some(50));
        assert_eq!(get_unicode_range_bit(0x30B1), Some(50));
        assert_eq!(get_unicode_range_bit(0x30FF), Some(50));

        assert_eq!(get_unicode_range_bit(0x10FFFD), Some(90));

        assert_eq!(get_unicode_range_bit(0x30000), None);
        assert_eq!(get_unicode_range_bit(0x110000), None);
    }
}
