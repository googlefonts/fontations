//! Contexts for initialization and hinting.

use raw::{FontRef, TableProvider};

use super::super::unscaled;
use crate::{
    charmap::Charmap, outline::OutlineGlyphCollection, prelude::NormalizedCoord, MetadataProvider,
};

const INLINE_OUTLINE_POINTS: usize = 64;

/// Common support for initializing autohinting state.
pub(super) struct InitContext<'a> {
    pub font: FontRef<'a>,
    pub coords: &'a [NormalizedCoord],
    pub units_per_em: u16,
    pub charmap: Charmap<'a>,
    pub glyphs: OutlineGlyphCollection<'a>,
    pub outline: &'a mut unscaled::UnscaledOutlineBuf<INLINE_OUTLINE_POINTS>,
}

impl<'a> InitContext<'a> {
    pub fn new(
        font: FontRef<'a>,
        coords: &'a [NormalizedCoord],
        outline: &'a mut unscaled::UnscaledOutlineBuf<INLINE_OUTLINE_POINTS>,
    ) -> Self {
        let units_per_em = font
            .head()
            .map(|head| head.units_per_em())
            .unwrap_or_default();
        let charmap = font.charmap();
        let glyphs = font.outline_glyphs();
        Self {
            font,
            coords,
            units_per_em,
            charmap,
            glyphs,
            outline,
        }
    }
}
