use super::{source::*, Context, Error, Hinting, NormalizedCoord, PathSink, Result, Variation};

use read_fonts::{
    types::{Fixed, GlyphId, Tag},
    TableProvider,
};

use core::{borrow::Borrow, str::FromStr};

/// Builder for configuring a glyph scaler.
pub struct ScalerBuilder<'a> {
    context: &'a mut Context,
    font_id: Option<u64>,
    size: f32,
    hint: Option<Hinting>,
}

impl<'a> ScalerBuilder<'a> {
    /// Creates a new builder for configuring a scaler with the given context.
    pub fn new(context: &'a mut Context) -> Self {
        context.coords.clear();
        context.variations.clear();
        Self {
            context,
            font_id: None,
            size: 0.0,
            hint: None,
        }
    }

    /// Sets a unique font identifier for hint state caching.
    pub fn font_id(mut self, font_id: Option<u64>) -> Self {
        self.font_id = font_id;
        self
    }

    /// Sets the font size in pixels per em units.
    ///
    /// A size of 0.0 will disable scaling and result in glyphs defined in font units.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size.abs();
        self
    }

    /// Sets the hinting mode.
    pub fn hint(mut self, hint: Option<Hinting>) -> Self {
        self.hint = hint;
        self
    }

    /// Specifies a variation with a set of normalized coordinates.
    ///
    /// This will clear any variations specified with the variations method.
    pub fn coords<I>(self, coords: I) -> Self
    where
        I: IntoIterator,
        I::Item: Borrow<NormalizedCoord>,
    {
        self.context.variations.clear();
        self.context.coords.clear();
        self.context
            .coords
            .extend(coords.into_iter().map(|v| *v.borrow()));
        self
    }

    /// Adds the sequence of variation settings. This will clear any variations
    /// specified as normalized coordinates.
    pub fn variations<I>(self, variations: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Variation>,
    {
        self.context.coords.clear();
        self.context
            .variations
            .extend(variations.into_iter().map(|v| v.into()));
        self
    }

    /// Builds a scaler using the currently configured settings
    /// for the specified font.
    pub fn build(self, font: &impl TableProvider<'a>) -> Scaler<'a> {
        if !self.context.variations.is_empty() {
            if let Ok(fvar) = font.fvar() {
                let avar_mappings = font.avar().ok().map(|avar| avar.axis_segment_maps());
                let axis_count = fvar.axis_count() as usize;
                self.context
                    .coords
                    .resize(axis_count, NormalizedCoord::default());
                if let Ok(axes) = fvar.axes() {
                    for (i, (axis, dest_coord)) in
                        axes.iter().zip(&mut self.context.coords).enumerate()
                    {
                        let tag = axis.axis_tag();
                        for variation in &self.context.variations {
                            if variation.tag == tag {
                                let mut coord =
                                    axis.normalize(Fixed::from_f64(variation.value as f64));
                                coord = avar_mappings
                                    .as_ref()
                                    .and_then(|mappings| mappings.get(i).transpose().ok())
                                    .flatten()
                                    .map(|mapping| mapping.apply(coord))
                                    .unwrap_or(coord);
                                let coord = coord.to_f64() as f32;
                                *dest_coord = NormalizedCoord::from_f32(coord);
                            }
                        }
                    }
                }
            }
        }
        let coords = &self.context.coords[..];
        let glyf = if let Ok(glyf) = glyf::Scaler::new(
            &mut self.context.glyf,
            font,
            self.font_id,
            self.size,
            self.hint,
            coords,
        ) {
            Some((glyf, &mut self.context.glyf_outline))
        } else {
            None
        };
        Scaler {
            outlines: Outlines { glyf },
        }
    }
}

/// Glyph scaler for a specific font and configuration.
pub struct Scaler<'a> {
    outlines: Outlines<'a>,
}

impl<'a> Scaler<'a> {
    /// Returns true if the scaler has a source for simple outlines.
    pub fn has_outlines(&self) -> bool {
        self.outlines.has_outlines()
    }

    /// Loads a simple outline for the specified glyph identifier and invokes the functions
    /// in the given sink for the sequence of path commands that define the outline.
    pub fn outline(&mut self, glyph_id: GlyphId, sink: &mut impl PathSink) -> Result<()> {
        self.outlines.outline(glyph_id, sink)
    }
}

/// Outline glyph scalers.
struct Outlines<'a> {
    glyf: Option<(glyf::Scaler<'a>, &'a mut glyf::Outline)>,
}

impl<'a> Outlines<'a> {
    /// Returns true if outlines are available.
    fn has_outlines(&self) -> bool {
        self.glyf.is_some()
    }

    /// Loads an outline for the specified glyph identifier and invokes the functions
    /// in the sink for the sequence of path commands that define the outline.   
    fn outline(&mut self, glyph_id: GlyphId, sink: &mut impl PathSink) -> Result<()> {
        if let Some((scaler, glyf_outline)) = &mut self.glyf {
            scaler.get_into(glyph_id, glyf_outline)?;
            glyf_outline.to_path(sink);
            return Ok(());
        }
        Err(Error::NoSources)
    }
}
