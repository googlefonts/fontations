//! The font data that the autohinter reads.

use super::{
    super::{DrawError, OutlineGlyph as FontRefGlyph, OutlineGlyphCollection},
    metrics::BlueOutlineBuf,
    outline::Outline,
    QuirksMode,
};
use crate::{
    attribute::Style, charmap::Charmap, instance::LocationRef, metrics::GlyphMetrics,
    prelude::Size, FontRef, MetadataProvider,
};
use core::cell::OnceCell;
use raw::{
    tables::{gdef::Gdef, gsub::Gsub},
    types::{F2Dot14, GlyphId},
    TableProvider,
};

/// Provides the font data the autohinter reads at one location in the
/// variation space.
///
/// The autohinter holds this as a trait object so that a single copy of the
/// autohinter serves every kind of font.
pub(crate) trait AutohintContext {
    /// Returns the normalized variation coordinates of the location.
    fn coords(&self) -> &[F2Dot14];

    /// Returns the number of glyphs that can be assigned a style.
    fn glyph_count(&self) -> u32;

    fn units_per_em(&self) -> i32;

    fn style(&self) -> Style;

    fn is_fixed_pitch(&self) -> bool;

    /// Returns true if the character map is a symbol map, which yields no
    /// metrics.
    fn is_symbol(&self) -> bool;

    fn map(&self, ch: u32) -> Option<GlyphId>;

    /// Calls `f` with each character and its glyph, in character order.
    fn for_each_mapping(&self, f: &mut dyn FnMut(u32, GlyphId));

    /// Returns the substitution table, which is read only when shaping uses
    /// it.
    fn gsub(&self) -> Option<Gsub<'_>>;

    /// Returns the glyph definition table, which provides the variation
    /// store.
    fn gdef(&self) -> Option<Gdef<'_>>;

    /// Returns the unscaled advance width of `glyph`.
    fn advance_width(&self, glyph: GlyphId) -> Option<f32>;

    /// Fills `outline` with `glyph` and analyzes it.
    ///
    /// Returns whether the glyph is flagged as having overlapping contours or
    /// components, or `None` if there is no outline for `glyph`.
    fn fill_outline(
        &self,
        glyph: GlyphId,
        outline: &mut Outline,
        quirks: QuirksMode,
    ) -> Option<Result<bool, DrawError>>;

    /// Draws the unscaled points of `glyph` into `buf`, or returns `None` if
    /// there is no outline for `glyph`.
    fn draw_unscaled(
        &self,
        glyph: GlyphId,
        buf: &mut BlueOutlineBuf,
    ) -> Option<Result<i32, DrawError>>;
}

/// An autohinting context for a [`FontRef`].
///
/// Data other than the glyph being drawn is read on first use. A draw of one
/// glyph rarely needs the character map or the other outlines.
pub(crate) struct FontRefContext<'a> {
    font: FontRef<'a>,
    coords: &'a [F2Dot14],
    charmap: OnceCell<Charmap<'a>>,
    /// The glyph being drawn, already resolved by the caller.
    drawn: Option<&'a FontRefGlyph<'a>>,
    /// Outlines loaded by the caller, used in place of `outlines`.
    borrowed_outlines: Option<&'a OutlineGlyphCollection<'a>>,
    outlines: OnceCell<OutlineGlyphCollection<'a>>,
    glyph_metrics: OnceCell<GlyphMetrics<'a>>,
}

impl<'a> FontRefContext<'a> {
    pub fn new(font: &FontRef<'a>, coords: &'a [F2Dot14]) -> Self {
        Self {
            font: font.clone(),
            coords,
            charmap: OnceCell::new(),
            drawn: None,
            borrowed_outlines: None,
            outlines: OnceCell::new(),
            glyph_metrics: OnceCell::new(),
        }
    }

    /// Creates a context for drawing `glyph`.
    ///
    /// The caller has already resolved `glyph`, so it is not looked up again.
    pub fn for_glyph(glyph: &'a FontRefGlyph<'a>, coords: &'a [F2Dot14]) -> Self {
        Self {
            drawn: Some(glyph),
            ..Self::new(glyph.font(), coords)
        }
    }

    /// Creates a context that reuses outlines the caller has already loaded.
    pub fn with_outlines(
        font: &FontRef<'a>,
        outlines: &'a OutlineGlyphCollection<'a>,
        coords: &'a [F2Dot14],
    ) -> Self {
        Self {
            borrowed_outlines: Some(outlines),
            ..Self::new(font, coords)
        }
    }

    fn charmap(&self) -> &Charmap<'a> {
        self.charmap.get_or_init(|| self.font.charmap())
    }

    fn outlines(&self) -> &OutlineGlyphCollection<'a> {
        match self.borrowed_outlines {
            Some(outlines) => outlines,
            None => self.outlines.get_or_init(|| self.font.outline_glyphs()),
        }
    }

    /// Resolves `glyph`, using the glyph being drawn if it matches and the
    /// collection otherwise.
    fn resolve(&self, glyph: GlyphId) -> Option<FontRefGlyph<'a>> {
        match self.drawn {
            Some(drawn) if drawn.glyph_id() == glyph => Some(drawn.clone()),
            _ => self.outlines().get(glyph),
        }
    }
}

impl AutohintContext for FontRefContext<'_> {
    fn coords(&self) -> &[F2Dot14] {
        self.coords
    }

    fn glyph_count(&self) -> u32 {
        // The style table is sized by the glyph count, which a font can
        // overstate; it never needs more entries than there are outlines.
        self.font
            .selected_maxp()
            .map(|maxp| maxp.num_glyphs())
            .unwrap_or_default()
            .min(self.outlines().stored_glyph_count())
    }

    fn units_per_em(&self) -> i32 {
        self.font
            .head()
            .map(|head| head.units_per_em() as i32)
            .unwrap_or_default()
    }

    fn style(&self) -> Style {
        self.font.attributes().style
    }

    fn is_fixed_pitch(&self) -> bool {
        self.font
            .post()
            .map(|post| post.is_fixed_pitch() != 0)
            .unwrap_or_default()
    }

    fn is_symbol(&self) -> bool {
        self.charmap().is_symbol()
    }

    fn map(&self, ch: u32) -> Option<GlyphId> {
        self.charmap().map(ch)
    }

    fn for_each_mapping(&self, f: &mut dyn FnMut(u32, GlyphId)) {
        for (ch, glyph) in self.charmap().mappings() {
            f(ch, glyph);
        }
    }

    fn gsub(&self) -> Option<Gsub<'_>> {
        self.font.gsub().ok()
    }

    fn gdef(&self) -> Option<Gdef<'_>> {
        self.font.gdef().ok()
    }

    fn advance_width(&self, glyph: GlyphId) -> Option<f32> {
        self.glyph_metrics
            .get_or_init(|| {
                self.font
                    .glyph_metrics(Size::unscaled(), LocationRef::new(self.coords))
            })
            .advance_width(glyph)
    }

    fn fill_outline(
        &self,
        glyph: GlyphId,
        outline: &mut Outline,
        quirks: QuirksMode,
    ) -> Option<Result<bool, DrawError>> {
        let glyph = self.resolve(glyph)?;
        Some(
            outline
                .fill(&glyph, self.coords, quirks)
                .map(|_| glyph.has_overlaps().unwrap_or_default()),
        )
    }

    fn draw_unscaled(
        &self,
        glyph: GlyphId,
        buf: &mut BlueOutlineBuf,
    ) -> Option<Result<i32, DrawError>> {
        let glyph = self.resolve(glyph)?;
        Some(glyph.draw_unscaled(self.coords, None, buf))
    }
}
