//! Metrics for a font and its glyphs.

mod global;

pub use global::{GlobalMetrics, LineBox, LineExtents};

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
pub use scaled::{Scale, Scale26Dot6, ScaleF32, ScaledGlobalMetrics, ScaledGlyphMetrics};
