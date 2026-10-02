//! Metrics for a font and its glyphs.

use crate::tables::mvar::MvarInstance;
use types::{F48Dot16, Tag};

/// Returns a design-unit metric with its location's delta applied.
fn metric(value: i32, deltas: Option<&MvarInstance>, tag: Tag) -> F48Dot16 {
    F48Dot16::from_i32(value)
        + deltas
            .and_then(|deltas| deltas.get(tag))
            .unwrap_or_default()
}

mod global;
mod style;

pub use global::{LineBox, LineExtents, Metrics};
pub use style::{Decoration, ScriptMetrics, StyleMetrics};

// The per-glyph metrics are reached only through a font, so they exist only
// where it does.
#[cfg(feature = "experimental_font_api")]
mod glyph;
#[cfg(feature = "experimental_font_api")]
mod scaled;

#[cfg(feature = "experimental_font_api")]
pub(crate) use glyph::{empty as empty_glyph_metrics, RawGlyphMetrics};
#[cfg(feature = "experimental_font_api")]
pub use glyph::{GlyphExtents, GlyphMetrics};
#[cfg(feature = "experimental_font_api")]
pub use scaled::{
    Scale, Scale26Dot6, ScaleF32, ScaledGlyphMetrics, ScaledMetrics, ScaledStyleMetrics,
};
