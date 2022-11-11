//! The [VVAR (Vertical Metrics Variation)](https://docs.microsoft.com/en-us/typography/opentype/spec/vvar) table

use crate::variations::{DeltaSetIndex, DeltaSetIndexMap, ItemVariationStore};
use font_types::Tag;

/// 'VVAR'
pub const TAG: Tag = Tag::new(b"VVAR");

include!("../../generated/generated_vvar.rs");

impl<'a> Vvar<'a> {
    /// Returns the advance height delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn advance_height_delta(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
    ) -> Result<Fixed, ReadError> {
        let gid = glyph_id.to_u16();
        let ix = match self.advance_height_mapping() {
            Some(Ok(dsim)) => dsim.get(gid as u32)?,
            _ => DeltaSetIndex {
                outer: 0,
                inner: gid,
            },
        };
        let ivs = self.item_variation_store()?;
        ivs.compute_delta(ix, coords)
    }

    /// Returns the top side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn tsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        let gid = glyph_id.to_u16();
        let ix = match self.tsb_mapping() {
            Some(Ok(dsim)) => dsim.get(gid as u32)?,
            _ => return Err(ReadError::NullOffset),
        };
        let ivs = self.item_variation_store()?;
        ivs.compute_delta(ix, coords)
    }

    /// Returns the bottom side bearing delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn bsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        let gid = glyph_id.to_u16();
        let ix = match self.bsb_mapping() {
            Some(Ok(dsim)) => dsim.get(gid as u32)?,
            _ => return Err(ReadError::NullOffset),
        };
        let ivs = self.item_variation_store()?;
        ivs.compute_delta(ix, coords)
    }

    /// Returns the vertical origin delta for the specified glyph identifier and
    /// normalized variation coordinates.
    pub fn v_org_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
        let gid = glyph_id.to_u16();
        let ix = match self.v_org_mapping() {
            Some(Ok(dsim)) => dsim.get(gid as u32)?,
            _ => return Err(ReadError::NullOffset),
        };
        let ivs = self.item_variation_store()?;
        ivs.compute_delta(ix, coords)
    }
}
