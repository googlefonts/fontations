//! Metrics in a caller's own units.

use super::{GlobalMetrics, GlyphExtents, GlyphMetrics, LineBox, LineExtents};
use types::{BoundingBox, F26Dot6, F48Dot16, Fixed, GlyphId};

/// Converts a measurement in design units into a caller's own units.
///
/// Written by whatever owns scaling in a text stack: a shaper, or the bridge
/// between one and this crate. Callers that want design units read them from
/// [`GlyphMetrics`] and [`GlobalMetrics`] and never implement this.
///
/// Scaling is more than a multiplication, which is why it is a trait rather
/// than a factor this crate would apply. HarfBuzz scales each axis by its own
/// fixed point number, in a format the caller picks, and is particular about
/// how it rounds and in what order it applies things. An implementation keeps
/// all of that, while the rules for combining several measurements into one
/// metric stay here.
///
/// The arithmetic is on this trait rather than bounds on
/// [`Value`](Self::Value) for the same reason: an implementation decides what
/// an overflowing sum does, and which way a halved odd number goes. HarfBuzz
/// halves two ways within one function, so neither is safe to assume.
pub trait Scale {
    /// What a scaled measurement is expressed as.
    type Value: Copy;

    /// Returns `a` plus `b`.
    ///
    /// This and the two below take no scale. They describe
    /// [`Value`](Self::Value) itself, so every scale of that type answers
    /// them alike.
    fn add(a: Self::Value, b: Self::Value) -> Self::Value;

    /// Returns `a` less `b`.
    fn sub(a: Self::Value, b: Self::Value) -> Self::Value;

    /// Returns half of `value`.
    fn half(value: Self::Value) -> Self::Value;

    /// Scales a measurement along the x axis.
    fn scale_x(&self, value: F48Dot16) -> Self::Value;

    /// Scales a measurement along the y axis.
    ///
    /// Separate from [`scale_x`](Self::scale_x) because the two axes need
    /// not agree, and because a caller whose y runs down the page says so
    /// here.
    fn scale_y(&self, value: F48Dot16) -> Self::Value;

    /// Scales where a glyph's ink sits.
    ///
    /// One call rather than four, so that a scale running the other way up
    /// settles in one place what that means for a bearing and a size.
    fn scale_glyph_extents(&self, extents: GlyphExtents<F48Dot16>) -> GlyphExtents<Self::Value>;

    /// Scales a region given by its corners.
    ///
    /// One call for the same reason as [`scale_glyph_extents`](Self::scale_glyph_extents):
    /// a scale running the other way up decides here which corner ends up
    /// least.
    fn scale_rect(&self, bounds: BoundingBox<F48Dot16>) -> BoundingBox<Self::Value>;
}

/// Scales design units to 26.6 pixels, as FreeType does.
///
/// Sums saturate and halves round toward negative infinity.
///
/// FreeType scales whole design units, so a measurement carrying a fraction
/// is rounded to a unit before it is scaled. [`ScaleF32`] keeps the
/// fraction.
#[derive(Copy, Clone, Debug)]
pub struct Scale26Dot6 {
    x: Fixed,
    y: Fixed,
}

impl Scale26Dot6 {
    /// A scale with a size for each axis, which HarfBuzz keeps apart.
    pub fn new(x_ppem: f32, y_ppem: f32, units_per_em: u16) -> Self {
        Self {
            x: Self::factor(x_ppem, units_per_em),
            y: Self::factor(y_ppem, units_per_em),
        }
    }

    /// A scale mapping `units_per_em` design units onto `ppem` pixels.
    pub fn from_ppem(ppem: f32, units_per_em: u16) -> Self {
        Self::new(ppem, ppem, units_per_em)
    }

    /// The factor FreeType multiplies whole design units by.
    ///
    /// It folds the conversion to 1/64 pixel into itself, so that a
    /// 16.16 multiply against a whole design unit lands with 26.6 in its
    /// bits. That is a trick rather than a type, and it is the one FreeType
    /// plays.
    fn factor(ppem: f32, units_per_em: u16) -> Fixed {
        Fixed::from_bits((ppem * 64.0) as i32) / Fixed::from_bits(units_per_em.max(1) as i32)
    }

    fn scale(value: F48Dot16, factor: Fixed) -> F26Dot6 {
        F26Dot6::from_bits((Fixed::from_bits(value.to_i32()) * factor).to_bits())
    }
}

impl Scale for Scale26Dot6 {
    type Value = F26Dot6;

    fn add(a: F26Dot6, b: F26Dot6) -> F26Dot6 {
        a.saturating_add(b)
    }

    fn sub(a: F26Dot6, b: F26Dot6) -> F26Dot6 {
        a.saturating_sub(b)
    }

    fn half(value: F26Dot6) -> F26Dot6 {
        F26Dot6::from_bits(value.to_bits() >> 1)
    }

    fn scale_x(&self, value: F48Dot16) -> F26Dot6 {
        Self::scale(value, self.x)
    }

    fn scale_y(&self, value: F48Dot16) -> F26Dot6 {
        Self::scale(value, self.y)
    }

    fn scale_glyph_extents(&self, e: GlyphExtents<F48Dot16>) -> GlyphExtents<F26Dot6> {
        GlyphExtents {
            x_bearing: self.scale_x(e.x_bearing),
            y_bearing: self.scale_y(e.y_bearing),
            width: self.scale_x(e.width),
            height: self.scale_y(e.height),
        }
    }

    fn scale_rect(&self, b: BoundingBox<F48Dot16>) -> BoundingBox<F26Dot6> {
        BoundingBox {
            x_min: self.scale_x(b.x_min),
            y_min: self.scale_y(b.y_min),
            x_max: self.scale_x(b.x_max),
            y_max: self.scale_y(b.y_max),
        }
    }
}

/// Scales design units to `f32` pixels.
///
/// Nothing here rounds, so a measurement carrying a fraction keeps it, which
/// [`Scale26Dot6`] does not.
#[derive(Copy, Clone, Debug)]
pub struct ScaleF32 {
    x: f32,
    y: f32,
}

impl ScaleF32 {
    /// A scale with a size for each axis, which HarfBuzz keeps apart.
    pub fn new(x_ppem: f32, y_ppem: f32, units_per_em: u16) -> Self {
        let per_unit = |ppem: f32| ppem / units_per_em.max(1) as f32;
        Self {
            x: per_unit(x_ppem),
            y: per_unit(y_ppem),
        }
    }

    /// A scale mapping `units_per_em` design units onto `ppem` pixels.
    pub fn from_ppem(ppem: f32, units_per_em: u16) -> Self {
        Self::new(ppem, ppem, units_per_em)
    }
}

impl Scale for ScaleF32 {
    type Value = f32;

    fn add(a: f32, b: f32) -> f32 {
        a + b
    }

    fn sub(a: f32, b: f32) -> f32 {
        a - b
    }

    fn half(value: f32) -> f32 {
        value * 0.5
    }

    fn scale_x(&self, value: F48Dot16) -> f32 {
        value.to_f32() * self.x
    }

    fn scale_y(&self, value: F48Dot16) -> f32 {
        value.to_f32() * self.y
    }

    fn scale_glyph_extents(&self, e: GlyphExtents<F48Dot16>) -> GlyphExtents<f32> {
        GlyphExtents {
            x_bearing: self.scale_x(e.x_bearing),
            y_bearing: self.scale_y(e.y_bearing),
            width: self.scale_x(e.width),
            height: self.scale_y(e.height),
        }
    }

    fn scale_rect(&self, b: BoundingBox<F48Dot16>) -> BoundingBox<f32> {
        BoundingBox {
            x_min: self.scale_x(b.x_min),
            y_min: self.scale_y(b.y_min),
            x_max: self.scale_x(b.x_max),
            y_max: self.scale_y(b.y_max),
        }
    }
}

/// Measurements of a font as a whole, in a caller's own units.
///
/// A view rather than a set of scaled fields, so reading one measurement
/// does not compute the rest. What carries no unit, such as the glyph count,
/// is reported unchanged.
#[derive(Clone, Copy)]
pub struct ScaledGlobalMetrics<'a, S: Scale> {
    metrics: &'a GlobalMetrics,
    scale: S,
}

impl<'a, S: Scale> ScaledGlobalMetrics<'a, S> {
    pub(crate) fn new(metrics: &'a GlobalMetrics, scale: S) -> Self {
        Self { metrics, scale }
    }

    /// Returns the number of glyphs in the font.
    #[inline]
    pub fn num_glyphs(&self) -> u32 {
        self.metrics.num_glyphs
    }

    /// Returns the size of the font's design em.
    #[inline]
    pub fn units_per_em(&self) -> u16 {
        self.metrics.units_per_em
    }

    /// Returns the box enclosing every glyph in the font.
    #[inline]
    pub fn bounds(&self) -> BoundingBox<S::Value> {
        self.scale.scale_rect(self.metrics.bounds)
    }

    /// Returns the ascender, descender and gap for horizontal text.
    ///
    /// Resolved as [`GlobalMetrics::h_line`] resolves it, with each end
    /// scaled on its own so that a caller subtracting them gets the height
    /// the glyph metrics report for a font that stacks by this line.
    #[inline]
    pub fn h_line(&self) -> Option<LineBox<S::Value>> {
        self.metrics.h_line().map(|line| self.scale_line_y(line))
    }

    /// Returns the line `hhea` provides.
    #[inline]
    pub fn hhea_line(&self) -> Option<LineBox<S::Value>> {
        self.metrics.hhea_line.map(|line| self.scale_line_y(line))
    }

    /// Returns the typographic line `OS/2` provides.
    #[inline]
    pub fn typo_line(&self) -> Option<LineBox<S::Value>> {
        self.metrics.typo_line.map(|line| self.scale_line_y(line))
    }

    /// Returns whether the font asks for its typographic line to be read.
    #[inline]
    pub fn use_typo_metrics(&self) -> bool {
        self.metrics.use_typo_metrics
    }

    /// Returns the line `vhea` provides, where the font has one.
    ///
    /// A vertical line runs across the page, so its measurements scale along
    /// x where a horizontal line's scale along y.
    #[inline]
    pub fn vhea_line(&self) -> Option<LineBox<S::Value>> {
        self.metrics.vhea_line.map(|line| LineBox {
            ascender: self.scale.scale_x(line.ascender),
            descender: self.scale.scale_x(line.descender),
            line_gap: self.scale.scale_x(line.line_gap),
        })
    }

    /// Returns the height of a lowercase letter, where the font provides one.
    #[inline]
    pub fn x_height(&self) -> Option<S::Value> {
        self.scaled_y(self.metrics.x_height)
    }

    /// Returns the height of a capital letter, where the font provides one.
    #[inline]
    pub fn cap_height(&self) -> Option<S::Value> {
        self.scaled_y(self.metrics.cap_height)
    }

    /// Returns the widest advance in the font, where it provides one.
    #[inline]
    pub fn max_advance_width(&self) -> Option<S::Value> {
        self.metrics
            .max_advance_width
            .map(|value| self.scale.scale_x(value))
    }

    /// Returns the tallest advance in the font, where it provides one.
    #[inline]
    pub fn max_advance_height(&self) -> Option<S::Value> {
        self.scaled_y(self.metrics.max_advance_height)
    }

    /// Returns the average advance in the font, where it provides one.
    #[inline]
    pub fn average_char_width(&self) -> Option<S::Value> {
        self.metrics
            .average_char_width
            .map(|value| self.scale.scale_x(value))
    }

    /// Returns the line outside which the font asks not to be clipped.
    #[inline]
    pub fn win_line(&self) -> Option<LineExtents<S::Value>> {
        self.metrics.win_line.map(|line| self.scale_line(line))
    }

    #[inline]
    fn scale_line_y(&self, line: LineBox<F48Dot16>) -> LineBox<S::Value> {
        LineBox {
            ascender: self.scale.scale_y(line.ascender),
            descender: self.scale.scale_y(line.descender),
            line_gap: self.scale.scale_y(line.line_gap),
        }
    }

    #[inline]
    fn scale_line(&self, line: LineExtents<F48Dot16>) -> LineExtents<S::Value> {
        LineExtents {
            ascender: self.scale.scale_y(line.ascender),
            descender: self.scale.scale_y(line.descender),
        }
    }

    #[inline]
    fn scaled_y(&self, value: Option<F48Dot16>) -> Option<S::Value> {
        value.map(|value| self.scale.scale_y(value))
    }
}

impl GlobalMetrics {
    /// Returns these metrics in the units `scale` describes.
    #[inline]
    pub fn scaled<S: Scale>(&self, scale: S) -> ScaledGlobalMetrics<'_, S> {
        ScaledGlobalMetrics::new(self, scale)
    }
}

/// Measurements of individual glyphs, in a caller's own units.
///
/// Holds borrows and a scale, so it is as cheap to make as the metrics it
/// scales and can be taken per query. It remembers nothing between calls: a
/// caller wanting measurements kept holds them itself.
#[derive(Clone, Copy)]
pub struct ScaledGlyphMetrics<'a, S: Scale> {
    metrics: GlyphMetrics<'a>,
    scale: S,
    line: Option<LineExtents<S::Value>>,
}

impl<'a, S: Scale> ScaledGlyphMetrics<'a, S> {
    pub(crate) fn new(metrics: GlyphMetrics<'a>, scale: S) -> Self {
        Self {
            metrics,
            scale,
            line: None,
        }
    }

    /// Measures against a line the caller supplies rather than the font's own.
    ///
    /// A caller that reads its line from elsewhere supplies it here, so the
    /// metrics built on it agree with the rest of its layout. The line is in
    /// the same units as everything else this type reports, so nothing is
    /// converted back into design units.
    ///
    /// `None` leaves the font's own line in place, so a caller can pass
    /// whatever it happens to have without first checking.
    pub fn with_line_extents(self, line: Option<LineExtents<S::Value>>) -> Self {
        Self { line, ..self }
    }

    /// Returns the advance width of `glyph`.
    #[inline]
    pub fn h_advance(&self, glyph: GlyphId) -> S::Value {
        self.scale.scale_x(self.metrics.h_advance_exact(glyph))
    }

    /// Writes the advance width of each glyph to its slot, in order.
    #[inline]
    pub fn h_advance_batched<'o, V: 'o>(
        &self,
        convert: impl Fn(S::Value) -> V,
        glyphs: impl Iterator<Item = (GlyphId, &'o mut V)>,
    ) {
        self.metrics
            .h_advance_batched(|value| convert(self.scale.scale_x(value)), glyphs);
    }

    /// Returns the advance height of `glyph`.
    ///
    /// A font with no vertical metrics stacks its glyphs by the line, the
    /// only measurement here a caller can supply. One that has them is
    /// measured by what it says, and a supplied line changes nothing.
    #[inline]
    pub fn v_advance(&self, glyph: GlyphId) -> S::Value {
        // Only a font with no vertical metrics stacks by the line; one
        // that has them is read below, supplied line or not.
        match self.line_height_fallback() {
            Some(height) => height,
            None => self.scale.scale_y(self.metrics.v_advance_exact(glyph)),
        }
    }

    /// Writes the advance height of each glyph to its slot, in order.
    #[inline]
    pub fn v_advance_batched<'o, V: 'o>(
        &self,
        convert: impl Fn(S::Value) -> V,
        glyphs: impl Iterator<Item = (GlyphId, &'o mut V)>,
    ) {
        // Settled before the run, as the table to read is. Only a font
        // with no vertical metrics reaches this, and then all its glyphs
        // advance the same; one that has them is read below, supplied line
        // or not.
        if let Some(height) = self.line_height_fallback() {
            for (_, out) in glyphs {
                *out = convert(height);
            }
            return;
        }
        self.metrics
            .v_advance_batched(|value| convert(self.scale.scale_y(value)), glyphs);
    }

    /// The height a glyph takes from the line, where it takes one.
    ///
    /// `None` for two reasons that mean the same thing here: the font has
    /// vertical metrics of its own, so the line is not what its glyphs stack
    /// by; or it has no line either, which the unscaled metrics answer with
    /// an em. A supplied line applies only in the first case, so supplying
    /// one never overrides what a font says about itself.
    ///
    /// Each end is scaled before the two are subtracted, so a scale that
    /// rounds rounds them as a caller supplying its own line would, and the
    /// answer does not turn on whether one was supplied.
    #[inline]
    fn line_height_fallback(&self) -> Option<S::Value> {
        if self.metrics.states_vertical_advances() {
            return None;
        }
        let line = match self.line {
            Some(line) => line,
            None => {
                let from_font = self.metrics.line_extents()?;
                LineExtents {
                    ascender: self.scale.scale_y(from_font.ascender),
                    descender: self.scale.scale_y(from_font.descender),
                }
            }
        };
        Some(S::sub(line.ascender, line.descender))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Font;
    use types::GlyphId;

    /// Scales design units to pixels at a size, halving toward zero.
    #[derive(Clone, Copy)]
    struct Ppem {
        ppem: f32,
        upem: u16,
    }

    impl Scale for Ppem {
        type Value = f32;

        fn scale_x(&self, value: F48Dot16) -> f32 {
            value.to_f32() * self.ppem / self.upem as f32
        }

        fn scale_y(&self, value: F48Dot16) -> f32 {
            self.scale_x(value)
        }

        fn scale_glyph_extents(&self, extents: GlyphExtents<F48Dot16>) -> GlyphExtents<f32> {
            GlyphExtents {
                x_bearing: self.scale_x(extents.x_bearing),
                y_bearing: self.scale_y(extents.y_bearing),
                width: self.scale_x(extents.width),
                height: self.scale_y(extents.height),
            }
        }

        fn scale_rect(&self, b: BoundingBox<F48Dot16>) -> BoundingBox<f32> {
            BoundingBox {
                x_min: self.scale_x(b.x_min),
                y_min: self.scale_y(b.y_min),
                x_max: self.scale_x(b.x_max),
                y_max: self.scale_y(b.y_max),
            }
        }

        fn add(a: f32, b: f32) -> f32 {
            a + b
        }

        fn sub(a: f32, b: f32) -> f32 {
            a - b
        }

        fn half(value: f32) -> f32 {
            value / 2.0
        }
    }

    const STATIC: &[u8] = font_test_data::TINOS_SUBSET;
    const VERT: &[u8] = font_test_data::MPLUS1CODE_VERTICAL_SUBSET;

    fn scaled(data: &[u8], ppem: f32) -> (Font, Ppem) {
        let font = Font::new(data.to_vec(), 0).unwrap();
        let upem = font.global_metrics().units_per_em;
        (font, Ppem { ppem, upem })
    }

    fn line(ascender: f32, descender: f32) -> LineExtents<f32> {
        LineExtents {
            ascender,
            descender,
        }
    }

    /// Rounds every scaled value to a whole number, as a hinted scale does.
    #[derive(Clone, Copy)]
    struct Rounding(f32, u16);

    impl Scale for Rounding {
        type Value = f32;
        fn scale_x(&self, value: F48Dot16) -> f32 {
            (value.to_f32() * self.0 / self.1 as f32).round()
        }
        fn scale_y(&self, value: F48Dot16) -> f32 {
            self.scale_x(value)
        }
        fn scale_glyph_extents(&self, e: GlyphExtents<F48Dot16>) -> GlyphExtents<f32> {
            GlyphExtents {
                x_bearing: self.scale_x(e.x_bearing),
                y_bearing: self.scale_y(e.y_bearing),
                width: self.scale_x(e.width),
                height: self.scale_y(e.height),
            }
        }

        fn scale_rect(&self, b: BoundingBox<F48Dot16>) -> BoundingBox<f32> {
            BoundingBox {
                x_min: self.scale_x(b.x_min),
                y_min: self.scale_y(b.y_min),
                x_max: self.scale_x(b.x_max),
                y_max: self.scale_y(b.y_max),
            }
        }
        fn add(a: f32, b: f32) -> f32 {
            a + b
        }
        fn sub(a: f32, b: f32) -> f32 {
            a - b
        }
        fn half(value: f32) -> f32 {
            value / 2.0
        }
    }

    #[test]
    fn a_rounding_scale_rounds_the_ends_of_the_line_not_the_height() {
        // A font with no `vmtx` stacks by the line, and the height comes
        // from two ends. Scaling each before subtracting is what a caller
        // stating its own line does, so doing it the other way round would
        // answer differently depending on whether one was supplied. It shows
        // up only where the scale rounds, and then by a whole unit.
        let font = Font::new(STATIC.to_vec(), 0).unwrap();
        let upem = font.global_metrics().units_per_em;
        let from_font = font.global_metrics().h_line().unwrap().extents();
        let mut rounded_apart = 0;
        for ppem in [11.0f32, 12.0, 13.0, 14.0, 16.0, 19.0, 24.0] {
            let scale = Rounding(ppem, upem);
            let metrics = font.glyph_metrics().scaled(scale);
            let ends = LineExtents {
                ascender: scale.scale_y(from_font.ascender),
                descender: scale.scale_y(from_font.descender),
            };
            let from_ends = Rounding::sub(ends.ascender, ends.descender);
            assert_eq!(
                metrics.v_advance(GlyphId::new(1)),
                from_ends,
                "at {ppem}ppem"
            );
            // And stating the same line back gives the same answer.
            assert_eq!(
                metrics
                    .with_line_extents(Some(ends))
                    .v_advance(GlyphId::new(1)),
                from_ends,
                "at {ppem}ppem, supplied"
            );
            // The other way round: subtract in design units, scale once.
            if scale.scale_y(from_font.ascender - from_font.descender) != from_ends {
                rounded_apart += 1;
            }
        }
        assert!(
            rounded_apart > 0,
            "no size here rounds the two ways apart, so this proves nothing"
        );
    }

    #[test]
    fn the_scaled_line_is_the_one_the_glyph_metrics_stack_by() {
        // The reason both views exist. A caller reading the line from the
        // global metrics and subtracting its ends must land on the height
        // the glyph metrics give a font that stacks by that line, or the two
        // disagree about the same font at the same size.
        let font = Font::new(STATIC.to_vec(), 0).unwrap();
        let upem = font.global_metrics().units_per_em;
        for ppem in [11.0f32, 12.0, 13.0, 16.0, 19.0, 24.0] {
            let scale = Rounding(ppem, upem);
            let line = font.global_metrics().scaled(scale).h_line().unwrap();
            let height = Rounding::sub(line.ascender, line.descender);
            let glyphs = font.glyph_metrics().scaled(scale);
            for gid in (0..font.num_glyphs()).map(GlyphId::new) {
                assert_eq!(glyphs.v_advance(gid), height, "glyph {gid} at {ppem}ppem");
            }
        }
    }

    #[test]
    fn every_measurement_is_the_unscaled_one_through_the_scale() {
        // One case per field of `GlobalMetrics`, so that a field added there
        // without one here is a gap someone has to notice.
        let font = Font::new(STATIC.to_vec(), 0).unwrap();
        let global = font.global_metrics();
        let scale = Ppem {
            ppem: 16.0,
            upem: global.units_per_em,
        };
        let scaled = global.scaled(scale);
        let y = |line: LineBox<F48Dot16>| LineBox {
            ascender: scale.scale_y(line.ascender),
            descender: scale.scale_y(line.descender),
            line_gap: scale.scale_y(line.line_gap),
        };
        let x = |line: LineBox<F48Dot16>| LineBox {
            ascender: scale.scale_x(line.ascender),
            descender: scale.scale_x(line.descender),
            line_gap: scale.scale_x(line.line_gap),
        };
        assert_eq!(scaled.bounds(), scale.scale_rect(global.bounds));
        assert_eq!(scaled.hhea_line(), global.hhea_line.map(y));
        assert_eq!(scaled.typo_line(), global.typo_line.map(y));
        assert_eq!(scaled.vhea_line(), global.vhea_line.map(x));
        assert_eq!(scaled.h_line(), global.h_line().map(y));
        assert_eq!(
            scaled.win_line(),
            global.win_line.map(|l| LineExtents {
                ascender: scale.scale_y(l.ascender),
                descender: scale.scale_y(l.descender),
            })
        );
        assert_eq!(scaled.x_height(), global.x_height.map(|v| scale.scale_y(v)));
        assert_eq!(
            scaled.cap_height(),
            global.cap_height.map(|v| scale.scale_y(v))
        );
        assert_eq!(
            scaled.max_advance_width(),
            global.max_advance_width.map(|v| scale.scale_x(v))
        );
        assert_eq!(
            scaled.max_advance_height(),
            global.max_advance_height.map(|v| scale.scale_y(v))
        );
        assert_eq!(
            scaled.average_char_width(),
            global.average_char_width.map(|v| scale.scale_x(v))
        );
        // What carries no unit is reported unchanged.
        assert_eq!(scaled.units_per_em(), global.units_per_em);
        assert_eq!(scaled.num_glyphs(), global.num_glyphs);
        assert_eq!(scaled.use_typo_metrics(), global.use_typo_metrics);
        // And this font provides enough for the comparison to mean something.
        assert!(scaled.hhea_line().is_some() && scaled.typo_line().is_some());
    }

    #[test]
    fn the_clipping_line_reports_its_descent_as_a_position() {
        // `OS/2` gives the descent as a positive number below the baseline.
        // Reporting it as a position keeps the pair readable as a line.
        let font = Font::new(STATIC.to_vec(), 0).unwrap();
        let global = font.global_metrics();
        let win = global
            .win_line
            .expect("this font provides clipping metrics");
        assert!(win.ascender > F48Dot16::ZERO);
        assert!(win.descender < F48Dot16::ZERO);
        let scale = Ppem {
            ppem: 16.0,
            upem: global.units_per_em,
        };
        let scaled = global.scaled(scale).win_line().unwrap();
        assert_eq!(scaled.ascender, scale.scale_y(win.ascender));
        assert_eq!(scaled.descender, scale.scale_y(win.descender));
    }

    #[test]
    fn the_two_scales_agree_within_what_each_can_hold() {
        // The same font at the same size, in two number types. They differ by
        // what each can represent, so the coarser rounds the finer rather
        // than disagreeing with it.
        let font = Font::new(STATIC.to_vec(), 0).unwrap();
        let upem = font.global_metrics().units_per_em;
        let coarse = font
            .glyph_metrics()
            .scaled(Scale26Dot6::from_ppem(16.0, upem));
        let fine = font.glyph_metrics().scaled(ScaleF32::from_ppem(16.0, upem));
        for gid in (0..font.num_glyphs()).map(GlyphId::new) {
            // 26.6 resolves 1/64 pixel, so it lands within one of those.
            let apart = (coarse.h_advance(gid).to_f32() - fine.h_advance(gid)).abs();
            assert!(apart < 1.0 / 32.0, "glyph {gid} is {apart} apart");
        }
    }

    #[test]
    fn a_scale_with_a_size_per_axis_uses_each_in_its_own_direction() {
        // The axes are apart because HarfBuzz keeps them apart, so a scale
        // built from two factors has to use the right one in each direction.
        let font = Font::new(VERT.to_vec(), 0).unwrap();
        let upem = font.global_metrics().units_per_em;
        let em = upem as f32;
        let wide = ScaleF32::new(em * 2.0, em, upem);
        let tall = ScaleF32::new(em, em * 2.0, upem);
        let plain = ScaleF32::new(em, em, upem);
        let gid = GlyphId::new(1);
        let (w, t, p) = (
            font.glyph_metrics().scaled(wide),
            font.glyph_metrics().scaled(tall),
            font.glyph_metrics().scaled(plain),
        );
        assert_eq!(w.h_advance(gid), p.h_advance(gid) * 2.0);
        assert_eq!(w.v_advance(gid), p.v_advance(gid));
        assert_eq!(t.v_advance(gid), p.v_advance(gid) * 2.0);
        assert_eq!(t.h_advance(gid), p.h_advance(gid));
    }

    #[test]
    fn freetype_scales_whole_design_units() {
        // What separates the two: FreeType multiplies a whole design unit,
        // so a measurement carrying a fraction loses it before the multiply.
        // A varied advance carries one, which is why the other exists.
        let upem = 1000;
        let fraction = F48Dot16::from_f64(10.5);
        assert_eq!(
            Scale26Dot6::from_ppem(upem as f32, upem).scale_x(fraction),
            F26Dot6::from_f64(11.0)
        );
        assert_eq!(
            ScaleF32::from_ppem(upem as f32, upem).scale_x(fraction),
            10.5
        );
    }

    #[test]
    fn halving_rounds_toward_negative_infinity_in_26_6() {
        // Documented, because HarfBuzz halves two ways within one function
        // and a caller writing its own needs to know which it is getting.
        assert_eq!(
            Scale26Dot6::half(F26Dot6::from_bits(-3)),
            F26Dot6::from_bits(-2)
        );
        assert_eq!(ScaleF32::half(-3.0), -1.5);
    }

    #[test]
    fn a_scaled_advance_is_the_design_unit_one_through_the_scale() {
        let (font, scale) = scaled(STATIC, 16.0);
        let metrics = font.glyph_metrics();
        let scaled = metrics.scaled(scale);
        for gid in (0..font.num_glyphs()).map(GlyphId::new) {
            assert_eq!(
                scaled.h_advance(gid),
                scale.scale_x(metrics.h_advance_exact(gid)),
                "glyph {gid}"
            );
        }
    }

    #[test]
    fn a_stated_line_measures_a_font_that_states_none() {
        // This font has no `vmtx`, so its glyphs stack by the line.
        let (font, scale) = scaled(STATIC, 16.0);
        let metrics = font.glyph_metrics().scaled(scale);
        let supplied = metrics.with_line_extents(Some(line(12.0, -4.0)));
        let gid = GlyphId::new(1);
        assert_eq!(supplied.v_advance(gid), 16.0);
        assert!(metrics.v_advance(gid) != 16.0);
    }

    #[test]
    fn a_font_that_states_its_own_ignores_the_line() {
        // This one has `vmtx`, so the line is not what its glyphs stack by,
        // and stating one changes nothing.
        let (font, scale) = scaled(VERT, 16.0);
        let metrics = font.glyph_metrics().scaled(scale);
        let supplied = metrics.with_line_extents(Some(line(99.0, -99.0)));
        for gid in (0..font.num_glyphs()).map(GlyphId::new) {
            assert_eq!(
                metrics.v_advance(gid),
                supplied.v_advance(gid),
                "glyph {gid}"
            );
        }
    }

    #[test]
    fn stating_nothing_is_stating_nothing() {
        let (font, scale) = scaled(STATIC, 16.0);
        let metrics = font.glyph_metrics().scaled(scale);
        let gid = GlyphId::new(1);
        assert_eq!(
            metrics.with_line_extents(None).v_advance(gid),
            metrics.v_advance(gid)
        );
    }

    #[test]
    fn a_batch_agrees_with_one_at_a_time() {
        for (data, supplied) in [
            (STATIC, None),
            (STATIC, Some(line(12.0, -4.0))),
            (VERT, None),
            (VERT, Some(line(12.0, -4.0))),
        ] {
            let (font, scale) = scaled(data, 16.0);
            let metrics = font
                .glyph_metrics()
                .scaled(scale)
                .with_line_extents(supplied);
            let gids: Vec<_> = (0..font.num_glyphs()).map(GlyphId::new).collect();
            let mut h = vec![0.0f32; gids.len()];
            let mut v = vec![0.0f32; gids.len()];
            metrics.h_advance_batched(|value| value, gids.iter().copied().zip(h.iter_mut()));
            metrics.v_advance_batched(|value| value, gids.iter().copied().zip(v.iter_mut()));
            for (i, gid) in gids.iter().enumerate() {
                assert_eq!(metrics.h_advance(*gid), h[i], "h, glyph {gid}");
                assert_eq!(metrics.v_advance(*gid), v[i], "v, glyph {gid}");
            }
        }
    }
}
