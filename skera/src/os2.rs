//! impl subset() for OS/2
use crate::serialize::Serializer;
use crate::SubsetFlags;
use crate::{Plan, Subset, SubsetError};
use skrifa::Tag;
use std::cmp::Ordering;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::os2::{Os2, OS2_UNICODE_RANGES},
        FontRef, TopLevelTable,
    },
    FontBuilder,
};

// reference: subset() for OS/2 in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-os2-table.hh#L229
impl Subset for Os2<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        s.embed_bytes(self.offset_data().as_bytes())
            .map_err(|_| SubsetError::SubsetTableError(Os2::TAG))?;

        if !plan.normalized_coords.is_empty() {
            // I guess technically we should update this even if we aren't instantiating.
            // But Harfbuzz doesn't, so I won't either.
            let avg_char_width = calc_avg_char_width(plan.hmtx_map.borrow().values());
            s.copy_assign(
                self.shape().x_avg_char_width_byte_range().start,
                avg_char_width,
            );
        }

        if let Some(wght) = plan.user_axes_location.get(&Tag::new(b"wght")) {
            s.copy_assign(
                self.shape().us_weight_class_byte_range().start,
                wght.middle.clamp(1.0, 1000.0) as u16,
            );
        }
        if let Some(wdth) = plan.user_axes_location.get(&Tag::new(b"wdth")) {
            s.copy_assign(
                self.shape().us_width_class_byte_range().start,
                map_wdth_to_widthclass(wdth.middle).round() as u16,
            );
        }

        let us_first_char_index: u16 = plan.os2_info.min_cmap_codepoint.min(0xFFFF) as u16;
        s.copy_assign(
            self.shape().us_first_char_index_byte_range().start,
            us_first_char_index,
        );

        let us_last_char_index: u16 = plan.os2_info.max_cmap_codepoint.min(0xFFFF) as u16;
        s.copy_assign(
            self.shape().us_last_char_index_byte_range().start,
            us_last_char_index,
        );

        if !plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES)
        {
            update_unicode_ranges(&plan.unicodes, s.get_mut_data(42..58).unwrap());
        }
        Ok(())
    }
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

fn calc_avg_char_width<'a>(hmtx_map: impl Iterator<Item = &'a (u16, i16)>) -> u16 {
    let mut total_width: i32 = 0;
    let mut count: i32 = 0;
    for (aw, _) in hmtx_map {
        if *aw > 0 {
            total_width += *aw as i32;
            count += 1;
        }
    }
    if count > 0 {
        (total_width as f32 / count as f32).round() as u16
    } else {
        0
    }
}

fn map_wdth_to_widthclass(width: f64) -> f64 {
    if width < 50.0 {
        return 1.0;
    }
    if width > 200.0 {
        return 9.0;
    }

    let ratio = (width - 50.0) / 12.5;
    let mut a = ratio.floor() as i32;
    let mut b = ratio.ceil() as i32;

    /* follow this maping:
     * https://docs.microsoft.com/en-us/typography/opentype/spec/os2#uswidthclass
     */
    if b <= 6 {
        // 50-125
        if a == b {
            return a as f64 + 1.0;
        }
    } else if b == 7 {
        // no mapping for 137.5
        a = 6;
        b = 8;
    } else if b == 8 {
        if a == b {
            return 8.0;
        }; // 150
        a = 6;
    } else {
        if a == b && a == 12 {
            return 9.0;
        }; //200
        b = 12;
        a = 8;
    }

    let va = 50.0 + a as f64 * 12.5;
    let vb = 50.0 + b as f64 * 12.5;

    let mut ret = a as f64 + (width - va) / (vb - va);
    if a <= 6 {
        ret += 1.0;
    }
    ret
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
