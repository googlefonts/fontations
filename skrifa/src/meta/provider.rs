pub use read_fonts as raw;
use super::metrics::{GlyphMetrics, Metrics};
use crate::{NormalizedCoord, NormalizedCoords, Size};

/// Interface for types that can provide font metadata.
pub trait MetadataProvider<'a>: raw::TableProvider<'a> + Sized {
    /// Returns the global font metrics for the specified size and normalized variation
    /// coordinates.
    fn metrics(&self, size: Size, coords: NormalizedCoords<'a>) -> Metrics {
        Metrics::new(self, size, coords)
    }

    /// Returns the glyph specific metrics for the specified size and normalized variation
    /// coordinates.
    fn glyph_metrics(&self, size: Size, coords: NormalizedCoords<'a>) -> GlyphMetrics<'a> {
        GlyphMetrics::new(self, size, coords)
    }
}

/// Blanket implementation of `MetadataProvider` for any type that implements
/// `TableProvider`.
impl<'a, T> MetadataProvider<'a> for T where T: raw::TableProvider<'a> {}
