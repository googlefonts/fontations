use super::metrics::{GlyphMetrics, Metrics};
use crate::NormalizedCoord;

/// Interface for types that can provide font metadata.
pub trait MetadataProvider<'a>: raw::TableProvider<'a> + Sized {
    /// Returns the global font metrics for the specified size in pixels per em units
    /// and normalized variation coordinates.
    ///
    /// If `size` is `None` or `Some(0.0)`, resulting metric values will be in font units.
    fn metrics(&self, size: Option<f32>, coords: &'a [NormalizedCoord]) -> Metrics {
        Metrics::from_font(self, size, coords)
    }

    /// Returns the glyph specific metrics for the specified size in pixels per em units
    /// and normalized variation coordinates.
    ///
    /// If `size` is `None` or `Some(0.0)`, resulting metric values will be in font units.
    fn glyph_metrics(&self, size: Option<f32>, coords: &'a [NormalizedCoord]) -> GlyphMetrics<'a> {
        GlyphMetrics::from_font(self, size, coords)
    }
}

/// Blanket implementation of `MetadataProvider` for any type that implements
/// `TableProvider`.
impl<'a, T> MetadataProvider<'a> for T where T: raw::TableProvider<'a> {}
