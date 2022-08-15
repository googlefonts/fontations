//! The [hmtx (Horizontal Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx) table

use font_types::Tag;

/// 'hmtx'
pub const TAG: Tag = Tag::new(b"hmtx");

include!("../../generated/generated_hmtx.rs");

fn n_glyphs_less_n_metrics(num_glyphs: u16, num_metrics: u16) -> usize {
    num_glyphs.saturating_sub(num_metrics) as usize
}
