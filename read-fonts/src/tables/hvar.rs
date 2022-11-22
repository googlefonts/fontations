//! The [HVAR (Horizontal Metrics Variation)](https://docs.microsoft.com/en-us/typography/opentype/spec/hvar) table

use super::variations::{self, DeltaSetIndexMap, ItemVariationStore};
use types::Tag;

/// 'HVAR'
pub const TAG: Tag = Tag::new(b"HVAR");

include!("../../generated/generated_hvar.rs");

impl<'a> Hvar<'a> {
    /// Returns the advance width delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn advance_width_delta(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
    ) -> Result<Fixed, ReadError> {
        variations::advance_delta(
            self.advance_width_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }

    /// Returns the left side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn lsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        variations::item_delta(
            self.lsb_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }

    /// Returns the left side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn rsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        variations::item_delta(
            self.rsb_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{test_data, FontRef, TableProvider};
    use types::{F2Dot14, Fixed, GlyphId};

    #[test]
    fn advance_deltas() {
        let font = FontRef::new(test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        let hvar = font.hvar().unwrap();
        let gid_a = GlyphId::new(1);
        assert_eq!(
            hvar.advance_width_delta(gid_a, &[F2Dot14::from_f32(-1.0)])
                .unwrap(),
            Fixed::from_f64(-113.0)
        );
        assert_eq!(
            hvar.advance_width_delta(gid_a, &[F2Dot14::from_f32(-0.75)])
                .unwrap(),
            Fixed::from_f64(-84.75)
        );
        assert_eq!(
            hvar.advance_width_delta(gid_a, &[F2Dot14::from_f32(-0.5)])
                .unwrap(),
            Fixed::from_f64(-56.5)
        );
        assert_eq!(
            hvar.advance_width_delta(gid_a, &[F2Dot14::from_f32(0.0)])
                .unwrap(),
            Fixed::from_f64(0.0)
        );
        assert_eq!(
            hvar.advance_width_delta(gid_a, &[F2Dot14::from_f32(0.5)])
                .unwrap(),
            Fixed::from_f64(29.5)
        );
        assert_eq!(
            hvar.advance_width_delta(gid_a, &[F2Dot14::from_f32(1.0)])
                .unwrap(),
            Fixed::from_f64(59.0)
        );
    }
}
