use super::{
    Context, Error, Hinting, NormalizedCoord, Pen, Result, ScalerMetrics, Size, UniqueId,
    VariationSetting,
};
use crate::outline;

use core::borrow::Borrow;
use read_fonts::{
    types::{Fixed, GlyphId},
    TableProvider,
};

/// Builder for configuring a glyph scaler.
///
/// See the [module level documentation](crate::scale#building-a-scaler)
/// for more detail.
pub struct ScalerBuilder<'a> {
    context: &'a mut Context,
    cache_key: Option<UniqueId>,
    size: Size,
    hint: Option<Hinting>,
}

impl<'a> ScalerBuilder<'a> {
    /// Creates a new builder for configuring a scaler with the given context.
    pub fn new(context: &'a mut Context) -> Self {
        context.coords.clear();
        context.variations.clear();
        Self {
            context,
            cache_key: None,
            size: Size::unscaled(),
            hint: None,
        }
    }

    /// Sets a unique font identifier for hint state caching. Specifying `None` will
    /// disable caching.
    pub fn cache_key(mut self, key: Option<UniqueId>) -> Self {
        self.cache_key = key;
        self
    }

    /// Sets the requested font size.
    ///
    /// The default value is `Size::unscaled()` and outlines will be generated
    /// in font units.
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Sets the hinting mode.
    ///
    /// Passing `None` will disable hinting.
    pub fn hint(mut self, hint: Option<Hinting>) -> Self {
        self.hint = hint;
        self
    }

    /// Specifies a variation with a set of normalized coordinates.
    ///
    /// This will clear any variations specified with the variations method.
    pub fn normalized_coords<I>(self, coords: I) -> Self
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

    /// Appends the given sequence of variation settings. This will clear any
    /// variations specified as normalized coordinates.
    ///
    /// This methods accepts any type which can be converted into an iterator
    /// that yields a sequence of values that are convertible to
    /// [`VariationSetting`]. Various conversions from tuples are provided.
    ///
    /// The following are all equivalent:
    ///
    /// ```
    /// # use skrifa::{scale::*, setting::VariationSetting, Tag};
    /// # let mut context = Context::new();
    /// # let builder = context.new_scaler();
    /// // slice of VariationSetting
    /// builder.variation_settings(&[
    ///     VariationSetting::new(Tag::new(b"wgth"), 720.0),
    ///     VariationSetting::new(Tag::new(b"wdth"), 50.0),
    /// ])
    /// # ; let builder = context.new_scaler();
    /// // slice of (Tag, f32)
    /// builder.variation_settings(&[(Tag::new(b"wght"), 720.0), (Tag::new(b"wdth"), 50.0)])
    /// # ; let builder = context.new_scaler();
    /// // slice of (&str, f32)
    /// builder.variation_settings(&[("wght", 720.0), ("wdth", 50.0)])
    /// # ;
    ///
    /// ```
    ///
    /// Iterators that yield the above types are also accepted.
    pub fn variation_settings<I>(self, settings: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<VariationSetting>,
    {
        self.context.coords.clear();
        self.context
            .variations
            .extend(settings.into_iter().map(|v| v.into()));
        self
    }

    /// Builds a scaler using the currently configured settings
    /// and the specified font.
    pub fn build(mut self, font: &impl TableProvider<'a>) -> Scaler<'a> {
        self.resolve_variations(font);
        let coords = &self.context.coords[..];
        let outlines = outline::OutlineCollection::new(font);
        let scaler = outlines
            .scaler(self.size, coords, self.hint)
            .unwrap_or_default();
        Scaler { outlines, scaler }
    }

    fn resolve_variations(&mut self, font: &impl TableProvider<'a>) {
        if self.context.variations.is_empty() {
            return; // nop
        }
        let Ok(fvar) = font.fvar() else {
            return; // nop
        };
        let Ok(axes) = fvar.axes() else {
            return; // nop
        };
        let avar_mappings = font.avar().ok().map(|avar| avar.axis_segment_maps());
        let axis_count = fvar.axis_count() as usize;
        self.context.coords.clear();
        self.context
            .coords
            .resize(axis_count, NormalizedCoord::default());
        for variation in &self.context.variations {
            // To permit non-linear interpolation, iterate over all axes to ensure we match
            // multiple axes with the same tag:
            // https://github.com/PeterConstable/OT_Drafts/blob/master/NLI/UnderstandingNLI.md
            // We accept quadratic behavior here to avoid dynamic allocation and with the assumption
            // that fonts contain a relatively small number of axes.
            for (i, axis) in axes
                .iter()
                .enumerate()
                .filter(|(_, axis)| axis.axis_tag() == variation.selector)
            {
                let coord = axis.normalize(Fixed::from_f64(variation.value as f64));
                let coord = avar_mappings
                    .as_ref()
                    .and_then(|mappings| mappings.get(i).transpose().ok())
                    .flatten()
                    .map(|mapping| mapping.apply(coord))
                    .unwrap_or(coord);
                self.context.coords[i] = coord.to_f2dot14();
            }
        }
    }
}

/// Glyph scaler for a specific font and configuration.
///
/// See the [module level documentation](crate::scale#getting-an-outline)
/// for more detail.
pub struct Scaler<'a> {
    outlines: outline::OutlineCollection<'a>,
    scaler: outline::Scaler,
}

impl<'a> Scaler<'a> {
    /// Returns the current set of normalized coordinates in use by the scaler.
    pub fn normalized_coords(&self) -> &[NormalizedCoord] {
        self.scaler.location().coords()
    }

    /// Returns true if the scaler has a source for simple outlines.
    pub fn has_outlines(&self) -> bool {
        self.outlines.format().is_some()
    }

    /// Loads a simple outline for the specified glyph identifier and invokes the functions
    /// in the given pen for the sequence of path commands that define the outline.
    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Result<ScalerMetrics> {
        let outline = self
            .outlines
            .get(glyph_id)
            .ok_or(Error::GlyphNotFound(glyph_id))?;
        outline
            .scale(&self.scaler, None, pen)
            .ok_or(Error::NoSources)
    }
}
