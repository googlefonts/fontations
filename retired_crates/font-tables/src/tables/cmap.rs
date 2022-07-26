//! The [cmap](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap) table

use font_types::Tag;

/// 'cmap'
pub const TAG: Tag = Tag::new(b"cmap");

include!("../../generated/generated_cmap.rs");

fn div_by_two(seg_count_x2: u16) -> usize {
    (seg_count_x2 / 2) as usize
}
