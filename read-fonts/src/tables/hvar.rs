//! The [HVAR (Horizontal Metrics Variation)](https://docs.microsoft.com/en-us/typography/opentype/spec/hvar) table

use crate::variations::{DeltaSetIndex, DeltaSetIndexMap, ItemVariationStore};
use font_types::Tag;

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
        let gid = glyph_id.to_u16();
        let ix = match self.advance_width_mapping() {
            Some(Ok(dsim)) => dsim.get(gid as u32)?,
            _ => DeltaSetIndex {
                outer: 0,
                inner: gid,
            },
        };
        let ivs = self.item_variation_store()?;
        ivs.compute_delta(ix, coords)
    }

    /// Returns the left side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn lsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        let gid = glyph_id.to_u16();
        let ix = match self.lsb_mapping() {
            Some(Ok(dsim)) => dsim.get(gid as u32)?,
            _ => return Err(ReadError::NullOffset),
        };
        let ivs = self.item_variation_store()?;
        ivs.compute_delta(ix, coords)
    }

    /// Returns the left side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn rsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        let gid = glyph_id.to_u16();
        let ix = match self.rsb_mapping() {
            Some(Ok(dsim)) => dsim.get(gid as u32)?,
            _ => return Err(ReadError::NullOffset),
        };
        let ivs = self.item_variation_store()?;
        ivs.compute_delta(ix, coords)
    }
}
