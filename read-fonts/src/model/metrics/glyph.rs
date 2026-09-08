//! Per-glyph metrics.

use crate::{
    model::{Font, FontKind},
    ps::{cs::CommandSink, type1::Type1Font},
    tables::hmtx::LongMetric,
    TableProvider,
};
use types::{F2Dot14, F48Dot16, Fixed, GlyphId};

/// Measurements of individual glyphs at one location.
///
/// Cheap to obtain, so it can be taken per query rather than held.
#[derive(Clone, Copy)]
pub struct GlyphMetrics<'a> {
    h_metrics: &'a RawGlyphMetrics<'a>,
    font: &'a Font,
    coords: &'a [F2Dot14],
    num_glyphs: u32,
    units_per_em: u16,
}

impl<'a> GlyphMetrics<'a> {
    /// Binds a font to the location its glyphs are measured at.
    #[inline]
    pub(crate) fn new(font: &'a Font, coords: &'a [F2Dot14]) -> Self {
        let global = font.global_metrics();
        Self {
            h_metrics: font.h_metrics(),
            font,
            coords,
            num_glyphs: global.num_glyphs,
            units_per_em: global.units_per_em,
        }
    }

    /// Returns the advance width of `glyph`, in design units.
    ///
    /// See [`h_advance_exact`](Self::h_advance_exact) for the width before
    /// it is narrowed to `f32`.
    #[inline]
    pub fn h_advance(&self, glyph: GlyphId) -> f32 {
        self.h_advance_exact(glyph).to_f32()
    }

    /// Returns the exact advance width of `glyph`, in design units.
    ///
    /// `hmtx` states a whole number of units and a location adds a fraction,
    /// so the sum carries one. How to round it is the caller's to decide, as
    /// is what to do with a location that carries an advance below zero.
    #[inline]
    pub fn h_advance_exact(&self, glyph: GlyphId) -> F48Dot16 {
        let mut width = F48Dot16::ZERO;
        self.h_advance_batched(|value| value, core::iter::once((glyph, &mut width)));
        width
    }

    /// Writes the advance width of each glyph to its slot, in order.
    ///
    /// Each width passes through `convert`, so a caller working in another
    /// number type writes into that type directly.
    #[inline]
    pub fn h_advance_batched<'o, V: 'o>(
        &self,
        convert: impl Fn(F48Dot16) -> V,
        glyphs: impl Iterator<Item = (GlyphId, &'o mut V)>,
    ) {
        let raw = self.h_metrics;
        if raw.is_empty() {
            // A Type 1 font states its widths in the charstrings rather than
            // a table, so it reaches here and is measured another way. The
            // test sits inside this branch so that a font with `hmtx`, which
            // is nearly all of them, never makes it.
            if let FontKind::Type1(font) = self.font.kind() {
                return self.h_advance_batched_type1(font, convert, glyphs);
            }
            // Otherwise the font states no widths at all, and every glyph
            // gets half an em; no location can move that.
            let half_em = F48Dot16::from_i32(self.units_per_em as i32 / 2);
            for (_, out) in glyphs {
                *out = convert(half_em);
            }
            return;
        }
        let coords = self.coords;
        if coords.is_empty() {
            return raw.run(self.num_glyphs, convert, glyphs);
        }
        self.h_advance_batched_varied(raw, coords, convert, glyphs)
    }

    /// The varied half of [`h_advance_batched`](Self::h_advance_batched).
    ///
    /// Out of line because reading `gvar` pulls in a large amount of code,
    /// whose size would otherwise be charged to every unvaried measurement.
    #[inline(never)]
    fn h_advance_batched_varied<'o, V: 'o>(
        &self,
        raw: &RawGlyphMetrics<'_>,
        coords: &[F2Dot14],
        convert: impl Fn(F48Dot16) -> V,
        glyphs: impl Iterator<Item = (GlyphId, &'o mut V)>,
    ) {
        // Ask the table that answers directly, and stop there if it does.
        if let Some(hvar) = self.font.hvar() {
            return raw.run_varied(
                self.num_glyphs,
                |gid| hvar.advance_delta(gid, coords).unwrap_or(F48Dot16::ZERO),
                convert,
                glyphs,
            );
        }
        // Without `HVAR`, the answer comes from the phantom points on the
        // outline: an advance spans the two horizontal ones, so a change in
        // it is a change in that span.
        if let (Some(gvar), Some((glyf, loca))) = (self.font.gvar(), self.font.glyf_loca()) {
            return raw.run_varied(
                self.num_glyphs,
                |gid| match gvar.phantom_point_deltas(glyf, loca, coords, gid) {
                    Ok(Some(deltas)) => (deltas[1].x - deltas[0].x).to_f48dot16(),
                    // A glyph the table says nothing about does not move,
                    // and neither does one it says something unreadable
                    // about. They are different states, not different
                    // answers.
                    _ => F48Dot16::ZERO,
                },
                convert,
                glyphs,
            );
        }
        // A location, but nothing stating what it changes.
        raw.run(self.num_glyphs, convert, glyphs)
    }

    /// The Type 1 half of [`h_advance_batched`](Self::h_advance_batched).
    ///
    /// A Type 1 charstring states its own width, so reading one means
    /// running it. `#[inline(never)]` because that has nothing in common with
    /// reading a table, and no `sfnt` should pay for its presence.
    #[inline(never)]
    fn h_advance_batched_type1<'o, V: 'o>(
        &self,
        font: &Type1Font,
        convert: impl Fn(F48Dot16) -> V,
        glyphs: impl Iterator<Item = (GlyphId, &'o mut V)>,
    ) {
        /// The width is all that is wanted, so the outline is discarded.
        struct WidthOnly;

        impl CommandSink for WidthOnly {
            fn move_to(&mut self, _x: Fixed, _y: Fixed) {}
            fn line_to(&mut self, _x: Fixed, _y: Fixed) {}
            fn curve_to(
                &mut self,
                _cx0: Fixed,
                _cy0: Fixed,
                _cx1: Fixed,
                _cy1: Fixed,
                _x: Fixed,
                _y: Fixed,
            ) {
            }
            fn close(&mut self) {}
        }

        // A charstring states its width in its own space, which the font
        // matrix maps to design units. No size is applied: that is the
        // caller's to do.
        let transform = font.transform(None);
        for (gid, out) in glyphs {
            // A charstring states its width before it draws anything, so one
            // that states none is malformed and measures as nothing.
            let width = font
                .evaluate_charstring(gid, &mut WidthOnly)
                .ok()
                .flatten()
                .map(|width| transform.transform_h_metric(width).to_f48dot16())
                .unwrap_or(F48Dot16::ZERO)
                .max(F48Dot16::ZERO);
            *out = convert(width);
        }
    }
}

/// The per-glyph records of `hmtx` or `vmtx`.
///
/// Both tables hold the same records, so one type reads either, and which
/// direction it describes is fixed when it is read.
///
/// Nothing here depends on a location, so every location shares one parse.
#[derive(Clone, Default, yoke::Yokeable)]
pub(crate) struct RawGlyphMetrics<'a> {
    metrics: &'a [LongMetric],
}

impl<'a> RawGlyphMetrics<'a> {
    /// Reads what `hmtx` states.
    pub(crate) fn from_hmtx(tables: &impl TableProvider<'a>) -> Self {
        Self {
            metrics: tables
                .hmtx()
                .map(|hmtx| hmtx.h_metrics())
                .unwrap_or_default(),
        }
    }

    /// Returns `true` if the table states no metrics.
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.metrics.is_empty()
    }

    /// Writes what the table states for each glyph.
    ///
    /// A stored advance is a `u16`, so unlike
    /// [`run_varied`](Self::run_varied) this can neither overflow the sum nor
    /// report anything below zero.
    #[inline]
    fn run<'o, V: 'o>(
        &self,
        num_glyphs: u32,
        convert: impl Fn(F48Dot16) -> V,
        glyphs: impl Iterator<Item = (GlyphId, &'o mut V)>,
    ) {
        for (gid, out) in glyphs {
            *out = convert(self.stored_advance(num_glyphs, gid));
        }
    }

    /// Writes each glyph's advance with `delta` applied.
    ///
    /// `delta` is settled before the run, leaving only the branches that turn
    /// on the glyph. The sum saturates rather than wrapping, but is not
    /// otherwise bounded: a font whose deltas say so can carry an advance
    /// below zero, and what to do about that is the caller's to decide.
    ///
    /// Glyphs beyond the end of the font measure zero here, as they do
    /// unvaried.
    #[inline]
    fn run_varied<'o, V: 'o>(
        &self,
        num_glyphs: u32,
        delta: impl Fn(GlyphId) -> F48Dot16,
        convert: impl Fn(F48Dot16) -> V,
        glyphs: impl Iterator<Item = (GlyphId, &'o mut V)>,
    ) {
        for (gid, out) in glyphs {
            // Glyphs beyond the end of the font measure zero, and no
            // delta applies to them. The index map clamps an out of range
            // glyph onto its last entry, so asking would answer with some
            // other glyph's delta.
            let advance = if gid.to_u32() < num_glyphs {
                self.stored_advance(num_glyphs, gid)
                    .saturating_add(delta(gid))
            } else {
                F48Dot16::ZERO
            };
            *out = convert(advance);
        }
    }

    /// Returns the advance the table states for `glyph`, before any location.
    ///
    /// A glyph past the end of the font advances by nothing. The last record
    /// covers every glyph after it, and clamping the index rather than
    /// falling back on a failed lookup keeps that bound provable, so a run
    /// compiles without a branch per glyph.
    ///
    /// `self.metrics` is never empty here: a caller settles that font before
    /// the run starts.
    #[inline]
    fn stored_advance(&self, num_glyphs: u32, glyph: GlyphId) -> F48Dot16 {
        if glyph.to_u32() >= num_glyphs {
            return F48Dot16::ZERO;
        }
        let index = (glyph.to_u32() as usize).min(self.metrics.len().saturating_sub(1));
        match self.metrics.get(index) {
            Some(metric) => F48Dot16::from_i32(metric.advance() as i32),
            None => F48Dot16::ZERO,
        }
    }
}

/// Returns the metrics of a font that states none.
pub(crate) fn empty() -> &'static RawGlyphMetrics<'static> {
    static EMPTY: RawGlyphMetrics<'static> = RawGlyphMetrics { metrics: &[] };
    &EMPTY
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{pen::NullPen, Font, FontBlob, FontInstance, NormalizedCoord},
        FontRef,
    };
    use alloc::{sync::Arc, vec, vec::Vec};
    use std::sync::Mutex;
    use types::Tag;

    const STATIC: &[u8] = font_test_data::TINOS_SUBSET;
    /// Has both `HVAR` and `gvar`, so it can answer either way.
    const VAR: &[u8] = font_test_data::VAZIRMATN_VAR;
    /// Eleven glyphs but one long metric, so ten of them are in the tail.
    const TAIL: &[u8] = font_test_data::MATERIAL_SYMBOLS_SUBSET;

    #[test]
    fn a_width_is_what_hmtx_stores() {
        // A font with a tail, so this covers both halves of `hmtx`.
        let font = Font::new(TAIL, 0).unwrap();
        let direct = FontRef::new(TAIL).unwrap();
        let hmtx = direct.hmtx().unwrap();
        let num_glyphs = direct.maxp().unwrap().num_glyphs();
        assert!(num_glyphs > 1);
        for gid in 0..num_glyphs as u32 {
            let expected = hmtx.advance(GlyphId::new(gid)).unwrap();
            assert_eq!(
                font.glyph_metrics().h_advance_exact(GlyphId::new(gid)),
                F48Dot16::from_i32(expected as i32)
            );
        }
    }

    #[test]
    fn a_glyph_past_the_end_advances_by_nothing() {
        // Deliberately unlike `Hmtx::advance`, which clamps to the last long
        // metric and so reports a width for a glyph the font does not have.
        // HarfBuzz returns zero once a font has metrics at all, and a shaper
        // handed a bad glyph id is better served by nothing than by whatever
        // the last real glyph happened to measure.
        let font = Font::new(STATIC, 0).unwrap();
        let direct = FontRef::new(STATIC).unwrap();
        let past = GlyphId::new(direct.maxp().unwrap().num_glyphs() as u32);

        assert_eq!(font.glyph_metrics().h_advance_exact(past), F48Dot16::ZERO);
        assert!(direct.hmtx().unwrap().advance(past).unwrap() > 0);
        assert_eq!(
            font.glyph_metrics().h_advance_exact(GlyphId::new(60000)),
            F48Dot16::ZERO
        );
    }

    #[test]
    fn a_font_with_no_widths_gives_every_glyph_half_an_em() {
        let font = Font::new(font_test_data::NAMES_ONLY, 0).unwrap();
        // That font has no `head` either, so the em is zero. The shape of
        // the answer is what matters.
        assert_eq!(
            font.glyph_metrics().h_advance_exact(GlyphId::new(1)),
            F48Dot16::ZERO
        );
    }

    #[test]
    fn glyphs_past_the_long_metrics_share_the_last_advance() {
        // `hmtx` stores a full metric for the first `numberOfHMetrics`
        // glyphs and a bare side bearing for the rest, which all advance by
        // the last stored width. That tail is how a font records a run of
        // glyphs of equal width, and it is most of this font.
        let font = Font::new(TAIL, 0).unwrap();
        let direct = FontRef::new(TAIL).unwrap();
        let num_glyphs = direct.maxp().unwrap().num_glyphs() as u32;
        let num_long = direct.hhea().unwrap().number_of_h_metrics() as u32;
        assert!(num_long < num_glyphs, "this font has no tail to test");

        let last = font
            .glyph_metrics()
            .h_advance_exact(GlyphId::new(num_long - 1));
        assert!(last > F48Dot16::ZERO);
        for gid in num_long..num_glyphs {
            assert_eq!(
                font.glyph_metrics().h_advance_exact(GlyphId::new(gid)),
                last,
                "glyph {gid} is in the tail and should share the last advance"
            );
        }
        // And the tail stops at the end of the font rather than running on.
        assert_eq!(
            font.glyph_metrics()
                .h_advance_exact(GlyphId::new(num_glyphs)),
            F48Dot16::ZERO
        );
    }

    #[test]
    fn glyphs_beyond_the_font_measure_zero_at_any_location() {
        // The delta set index map clamps an out of range glyph onto its
        // last entry, so without a check these would take the last real
        // glyph's delta and report an advance, and a negative one at that.
        let font = Font::new(VAR, 0).unwrap();
        let num_glyphs = font.num_glyphs();
        let instance = at(&font, -1.0);
        assert!(
            instance
                .glyph_metrics()
                .h_advance_exact(GlyphId::new(num_glyphs - 1))
                != font
                    .glyph_metrics()
                    .h_advance_exact(GlyphId::new(num_glyphs - 1)),
            "the last real glyph should move, or this proves nothing"
        );
        for gid in [num_glyphs, num_glyphs + 1, u32::MAX] {
            let gid = GlyphId::new(gid);
            assert_eq!(
                instance.glyph_metrics().h_advance_exact(gid),
                F48Dot16::ZERO,
                "glyph {gid} is beyond the font"
            );
            assert_eq!(font.glyph_metrics().h_advance_exact(gid), F48Dot16::ZERO);
        }
    }

    #[test]
    fn the_tables_are_parsed_once_for_the_font() {
        let font = Font::new(VAR, 0).unwrap();
        let first = font.h_metrics() as *const RawGlyphMetrics<'_>;
        for _ in 0..8 {
            assert!(core::ptr::eq(
                font.h_metrics() as *const RawGlyphMetrics<'_>,
                first
            ));
        }
        // And clones share it, since they share one `Arc<FontRepr>`.
        assert!(core::ptr::eq(
            font.clone().h_metrics() as *const RawGlyphMetrics<'_>,
            first
        ));
    }

    #[test]
    fn a_batch_agrees_with_one_at_a_time() {
        let font = Font::new(STATIC, 0).unwrap();
        let gids: Vec<_> = (0..16u32).map(GlyphId::new).collect();
        let mut batch = vec![F48Dot16::ZERO; 16];
        font.glyph_metrics()
            .h_advance_batched(|v| v, gids.iter().copied().zip(batch.iter_mut()));
        for (gid, expected) in gids.iter().zip(&batch) {
            assert_eq!(font.glyph_metrics().h_advance_exact(*gid), *expected);
        }
    }

    #[test]
    fn a_type1_glyph_is_measured_by_its_charstring() {
        // Type 1 states no metrics table, so a width that is neither zero
        // nor half an em can only have come from running the charstring.
        let font = Font::new(font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFB, 0).unwrap();
        let metrics = font.glyph_metrics();
        let half_em = F48Dot16::from_i32(font.units_per_em() as i32 / 2);
        assert!(font.num_glyphs() > 1);

        let widths: Vec<_> = (0..font.num_glyphs())
            .map(|gid| metrics.h_advance_exact(GlyphId::new(gid)))
            .collect();
        assert!(widths.iter().any(|width| *width > F48Dot16::ZERO));
        assert!(
            widths.iter().any(|width| *width != half_em),
            "every glyph reported the fallback, so no charstring was run"
        );
    }

    /// The fixture with its font matrix replaced, same length so the rest of
    /// the header is untouched. `xx` becomes 2, so every advance doubles.
    fn type1_with_a_stretched_matrix() -> Vec<u8> {
        const FROM: &[u8] = b"/FontMatrix [0.001 0 0 0.001 0 0 ]";
        const TO: &[u8] = b"/FontMatrix [0.002 0 0 0.001 0 0 ]";
        let base = font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFA;
        let at = base
            .windows(FROM.len())
            .position(|window| window == FROM)
            .expect("the fixture states the matrix this test rewrites");
        let mut data = base.to_vec();
        data[at..at + TO.len()].copy_from_slice(TO);
        data
    }

    #[test]
    fn a_type1_width_is_mapped_by_the_font_matrix() {
        // The charstring states a width in its own space, which the matrix
        // maps to design units. The fixture states the usual matrix, where
        // that mapping is the identity and a missing one would go unnoticed,
        // so this stretches it: every advance must double.
        let plain = Font::new(font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFA, 0).unwrap();
        let stretched = Font::new(type1_with_a_stretched_matrix(), 0).unwrap();
        let (a, b) = (plain.glyph_metrics(), stretched.glyph_metrics());
        assert_eq!(plain.num_glyphs(), stretched.num_glyphs());

        let mut stretched_any = false;
        for gid in (0..plain.num_glyphs()).map(GlyphId::new) {
            let width = a.h_advance_exact(gid);
            assert_eq!(
                b.h_advance_exact(gid),
                width.saturating_add(width),
                "glyph {gid}"
            );
            stretched_any |= width > F48Dot16::ZERO;
        }
        assert!(stretched_any, "no glyph had a width to stretch");
    }

    #[test]
    fn a_type1_advance_is_what_drawing_the_glyph_reports() {
        // `draw` is the reference: it maps the charstring's own space to
        // design units through the font matrix. Note that this font states
        // the usual matrix for an em of 1000, where that mapping is the
        // identity, so this pins agreement with `draw` rather than proving
        // the matrix is applied.
        let data = font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFA;
        let font = Font::new(data, 0).unwrap();
        let direct = crate::ps::type1::Type1Font::new(data).unwrap();
        let metrics = font.glyph_metrics();
        for gid in (0..font.num_glyphs()).map(GlyphId::new) {
            let expected = direct.draw(gid, None, &mut NullPen).ok().flatten();
            assert_eq!(
                metrics.h_advance(gid),
                expected.unwrap_or(0.0),
                "glyph {gid}"
            );
        }
    }

    #[test]
    fn the_narrowed_advance_agrees_with_the_exact_one() {
        for data in [STATIC, TAIL, VAR] {
            let font = Font::new(data, 0).unwrap();
            let metrics = font.glyph_metrics();
            for gid in (0..font.num_glyphs()).map(GlyphId::new) {
                assert_eq!(
                    metrics.h_advance(gid),
                    metrics.h_advance_exact(gid).to_f32()
                );
            }
        }
    }

    #[test]
    fn the_conversion_decides_what_a_caller_gets_back() {
        let font = Font::new(STATIC, 0).unwrap();
        let gids = [GlyphId::new(1), GlyphId::new(2)];
        let mut exact = [F48Dot16::ZERO; 2];
        let mut floats = [0.0f32; 2];
        font.glyph_metrics()
            .h_advance_batched(|v| v, gids.iter().copied().zip(exact.iter_mut()));
        font.glyph_metrics()
            .h_advance_batched(|v| v.to_f32(), gids.iter().copied().zip(floats.iter_mut()));
        for i in 0..2 {
            assert_eq!(floats[i], exact[i].to_f32());
        }
    }

    fn at(font: &Font, coord: f32) -> FontInstance {
        FontInstance::builder(font)
            .normalized_coords([NormalizedCoord::from_f32(coord)])
            .build()
    }

    /// A font whose tables arrive one at a time, recording what was asked
    /// for and withholding any tag in `hide`.
    fn callback_font(
        data: &'static [u8],
        asked: Arc<Mutex<Vec<Tag>>>,
        hide: &'static [&[u8; 4]],
    ) -> Font {
        let source: Arc<dyn Fn(Tag) -> Option<FontBlob> + Send + Sync> =
            Arc::new(move |tag: Tag| {
                asked.lock().unwrap().push(tag);
                if hide.iter().any(|hidden| Tag::new(hidden) == tag) {
                    return None;
                }
                let font = FontRef::new(data).ok()?;
                Some(FontBlob::from(font.table_data(tag)?.as_bytes().to_vec()))
            });
        Font::new(source, 0).unwrap()
    }

    fn asked_for(asked: &Arc<Mutex<Vec<Tag>>>, tag: &[u8; 4]) -> bool {
        asked.lock().unwrap().contains(&Tag::new(tag))
    }

    #[test]
    fn an_instance_varies_what_the_font_does_not() {
        let font = Font::new(VAR, 0).unwrap();
        let instance = at(&font, -1.0);
        let (varied, plain) = (instance.glyph_metrics(), font.glyph_metrics());
        let moved = (0..font.num_glyphs())
            .map(GlyphId::new)
            .any(|gid| varied.h_advance_exact(gid) != plain.h_advance_exact(gid));
        assert!(moved, "no glyph moved at the far end of the axis");
    }

    #[test]
    fn an_advance_keeps_the_fraction_a_location_adds() {
        // The stored width is whole and the delta is not, so a varied advance
        // should carry a fraction that rounding would have thrown away. If
        // nothing here is fractional the exactness is untested.
        let font = Font::new(VAR, 0).unwrap();
        let metrics = at(&font, -0.4);
        let metrics = metrics.glyph_metrics();
        let fractional = (0..font.num_glyphs())
            .map(GlyphId::new)
            .any(|gid| metrics.h_advance_exact(gid).to_bits() & 0xFFFF != 0);
        assert!(fractional, "no advance carried a fraction");
    }

    #[test]
    fn a_batch_agrees_with_one_at_a_time_at_a_location() {
        let font = Font::new(VAR, 0).unwrap();
        let instance = at(&font, -0.6);
        let metrics = instance.glyph_metrics();
        let gids: Vec<_> = (0..16u32).map(GlyphId::new).collect();
        let mut batch = vec![F48Dot16::ZERO; 16];
        metrics.h_advance_batched(|v| v, gids.iter().copied().zip(batch.iter_mut()));
        for (gid, expected) in gids.iter().zip(&batch) {
            assert_eq!(metrics.h_advance_exact(*gid), *expected);
        }
    }

    #[test]
    fn gvar_answers_the_same_as_hvar() {
        // The two rungs of the ladder describe the same font, so a font that
        // has both must measure the same either way. Withholding `HVAR`
        // forces the second rung, which exercises the real fallback rather
        // than a reimplementation of it.
        let font = Font::new(VAR, 0).unwrap();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let no_hvar = callback_font(VAR, asked.clone(), &[b"HVAR"]);
        for coord in [-1.0, -0.8, -0.25, 0.75, 1.0] {
            let (with, without) = (at(&font, coord), at(&no_hvar, coord));
            let (with, without) = (with.glyph_metrics(), without.glyph_metrics());
            for gid in (0..font.num_glyphs()).map(GlyphId::new) {
                assert_eq!(
                    with.h_advance_exact(gid),
                    without.h_advance_exact(gid),
                    "glyph {gid} disagrees at {coord}"
                );
            }
        }
        // And it really did take the other path.
        assert!(asked_for(&asked, b"gvar"));
    }

    #[test]
    fn a_default_location_reads_no_variation_table() {
        // The point of the whole arrangement: `gvar` is typically about half
        // a variable font, and on a platform that hands tables over one at a
        // time, asking for it means copying it. Static text must not.
        let asked = Arc::new(Mutex::new(Vec::new()));
        let font = callback_font(VAR, asked.clone(), &[]);
        let metrics = font.glyph_metrics();
        for gid in (0..8).map(GlyphId::new) {
            let _ = metrics.h_advance_exact(gid);
        }
        assert!(!asked_for(&asked, b"gvar"));
        assert!(!asked_for(&asked, b"HVAR"));
        assert!(asked_for(&asked, b"hmtx"), "but it should read hmtx");
    }

    #[test]
    fn an_instance_at_the_default_location_reads_no_variation_table() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let font = callback_font(VAR, asked.clone(), &[]);
        // All-zero coordinates are the default location, and an instance
        // collapses them to none.
        let instance = at(&font, 0.0);
        assert!(instance.normalized_coords().is_empty());
        let _ = instance.glyph_metrics().h_advance_exact(GlyphId::new(1));
        assert!(!asked_for(&asked, b"gvar"));
        assert!(!asked_for(&asked, b"HVAR"));
    }

    #[test]
    fn hvar_answering_keeps_gvar_and_the_outlines_unread() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let font = callback_font(VAR, asked.clone(), &[]);
        let _ = at(&font, -0.75)
            .glyph_metrics()
            .h_advance_exact(GlyphId::new(1));
        assert!(asked_for(&asked, b"HVAR"));
        for cold in [b"gvar", b"glyf", b"loca"] {
            assert!(
                !asked_for(&asked, cold),
                "HVAR answered, so {} should never have been copied",
                Tag::new(cold)
            );
        }
    }

    #[test]
    fn taking_the_metrics_reads_hmtx_but_nothing_that_varies() {
        // Every measurement needs `hmtx`, so it is read up front. What a
        // location changes is not: that waits until something asks.
        let asked = Arc::new(Mutex::new(Vec::new()));
        let font = callback_font(VAR, asked.clone(), &[]);
        let instance = at(&font, -0.75);
        let metrics = instance.glyph_metrics();
        assert!(asked_for(&asked, b"hmtx"));
        for cold in [b"HVAR", b"gvar", b"glyf", b"loca"] {
            assert!(!asked_for(&asked, cold), "read {}", Tag::new(cold));
        }
        // And measuring one glyph then reads only what answers.
        let _ = metrics.h_advance_exact(GlyphId::new(1));
        assert!(asked_for(&asked, b"HVAR"));
        assert!(!asked_for(&asked, b"gvar"));
    }
}
