//! Autohinting state for a font instance.

use super::{super::OutlineGlyphCollection, style::GlyphStyleMap};
use crate::MetadataProvider;
use alloc::sync::Arc;
use raw::TableProvider;

/// Set of derived glyph styles that are used for automatic hinting.
///
/// These are invariant per font so can be precomputed and reused for multiple
/// instances when requesting automatic hinting with [`Engine::Auto`](super::super::hint::Engine::Auto).
#[derive(Clone, Debug)]
pub struct GlyphStyles(Arc<GlyphStyleMap>);

impl GlyphStyles {
    /// Precomputes the full set of glyph styles for the given outlines.
    pub fn new(outlines: &OutlineGlyphCollection) -> Self {
        if let Some(outlines) = outlines.common() {
            let glyph_count = outlines
                .font
                .maxp()
                .map(|maxp| maxp.num_glyphs() as u32)
                .unwrap_or_default();
            Self(Arc::new(GlyphStyleMap::new(
                glyph_count,
                &outlines.font.charmap(),
            )))
        } else {
            Self(Default::default())
        }
    }
}
