use super::{
    attribute::Attributes,
    bitmap::BitmapStrikes,
    charmap::Charmap,
    color::ColorGlyphCollection,
    instance::{LocationRef, Size},
    metrics::{GlyphMetrics, Metrics},
    outline::OutlineGlyphCollection,
    string::{LocalizedStrings, StringId},
    variation::{AxisCollection, NamedInstanceCollection},
};

/// Interface for types that can provide font metadata.
pub trait MetadataProvider<'a>: raw::TableProvider<'a> + Sized {
    /// Returns the primary attributes for font classification-- stretch,
    /// style and weight.
    fn attributes(&self) -> Attributes {
        Attributes::new(self)
    }

    /// Returns the collection of variation axes.
    fn axes(&self) -> AxisCollection<'a> {
        AxisCollection::new(self)
    }

    /// Returns the collection of named variation instances.
    fn named_instances(&self) -> NamedInstanceCollection<'a> {
        NamedInstanceCollection::new(self)
    }

    /// Returns an iterator over the collection of localized strings for the
    /// given informational string identifier.
    fn localized_strings(&self, id: StringId) -> LocalizedStrings<'a> {
        LocalizedStrings::new(self, id)
    }

    /// Returns the global font metrics for the specified size and location in
    /// normalized variation space.
    fn metrics(&self, size: Size, location: impl Into<LocationRef<'a>>) -> Metrics {
        Metrics::new(self, size, location)
    }

    /// Returns the glyph specific metrics for the specified size and location
    /// in normalized variation space.
    fn glyph_metrics(&self, size: Size, location: impl Into<LocationRef<'a>>) -> GlyphMetrics<'a> {
        GlyphMetrics::new(self, size, location)
    }

    /// Returns the character to nominal glyph identifier mapping.
    fn charmap(&self) -> Charmap<'a> {
        Charmap::new(self)
    }

    /// Returns the collection of scalable glyph outlines.
    ///
    /// If the font contains multiple outline sources, this method prioritizes
    /// `glyf`, `CFF2` and `CFF` in that order. To select a specific outline
    /// source, use the [`OutlineGlyphCollection::with_format`] method.
    fn outline_glyphs(&self) -> OutlineGlyphCollection<'a> {
        OutlineGlyphCollection::new(self)
    }

    // Returns a collection of paintable color glyphs.
    fn color_glyphs(&self) -> ColorGlyphCollection<'a> {
        ColorGlyphCollection::new(self)
    }

    /// Returns the set of embedded bitmap strikes.
    ///
    /// If the font contains multiple bitmap strike sources, this method
    /// prioritizes `sbix`, `CBDT`, and `EBDT` in that order. To select a
    /// specific bitmap source, use the [`BitmapStrikes::with_format`] method.
    fn bitmap_strikes(&self) -> BitmapStrikes<'a> {
        BitmapStrikes::new(self)
    }
}

/// Blanket implementation of `MetadataProvider` for any type that implements
/// `TableProvider`.
impl<'a, T> MetadataProvider<'a> for T where T: raw::TableProvider<'a> {}
