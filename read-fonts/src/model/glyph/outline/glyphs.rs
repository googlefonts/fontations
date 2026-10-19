//! Glyph outlines loaded from the sources selected by a source order.

use super::{
    cff::CffGlyph,
    glyf::{GlyfContext, GlyfGlyph, Precision},
    type1::Type1Glyph,
    ComputedMetrics, ControlBoundsSink, DrawMemory, DrawOptions, PathSink, PointsFromPath,
    UnscaledOutlineSink,
};
use crate::model::Error;
use crate::{
    mem::with_scratch,
    model::{
        glyph::{outline::OutlineSource, GlyphSource, GlyphSourceOrder},
        metrics::GlyphExtents,
        Font, Kind,
    },
    ps::{cff::CffFontRef, type1::Type1Font},
    types::{BoundingBox, GlyphId},
};
use yoke::Yokeable;

/// Provides access to the glyph outlines in a font.
///
/// The sources used for drawing are selected when this is created. It is
/// cheap to create and copy because it only borrows tables that the font
/// has already parsed.
#[derive(Clone, Copy, Yokeable)]
pub struct OutlineGlyphs<'a> {
    font: &'a Font,
    base: Base<'a>,
}

/// The source used to draw glyphs.
#[derive(Clone, Copy)]
enum Base<'a> {
    None,
    Glyf(GlyfContext<'a>),
    Cff(&'a CffFontRef<'a>, OutlineSource),
    Type1(&'a Type1Font),
}

impl<'a> OutlineGlyphs<'a> {
    pub(crate) fn new(font: &'a Font, order: GlyphSourceOrder) -> Self {
        let mut base = Base::None;
        if let Kind::Type1(type1) = font.kind() {
            if order.contains(GlyphSource::Type1) {
                base = Base::Type1(type1);
            }
            return Self { font, base };
        }
        // A standalone CFF font has no tables, so it finds no `glyf` here,
        // only its charstrings as `CFF`.
        for source in order
            .iter()
            .filter_map(|source| OutlineSource::try_from(source).ok())
        {
            match source {
                OutlineSource::Glyf => {
                    if let Some(context) = GlyfContext::new(font) {
                        base = Base::Glyf(context);
                        break;
                    }
                }
                OutlineSource::Cff2 | OutlineSource::Cff => {
                    if let Some(cff) = font.charstrings(source) {
                        base = Base::Cff(cff, source);
                        break;
                    }
                }
                _ => {}
            }
        }
        Self { font, base }
    }

    /// Returns the font that contains these outlines.
    pub fn font(&self) -> &'a Font {
        self.font
    }

    /// Returns the table that provides the outlines.
    ///
    /// Returns `None` if the font has no outlines from the requested sources.
    pub fn source(&self) -> Option<OutlineSource> {
        match self.base {
            Base::None => None,
            Base::Glyf(_) => Some(OutlineSource::Glyf),
            Base::Cff(_, source) => Some(source),
            Base::Type1(_) => Some(OutlineSource::Type1),
        }
    }

    /// Returns the `CFF` or `CFF2` font used to draw the glyphs.
    ///
    /// Returns `None` if the base source is not `CFF` or `CFF2`.
    #[doc(hidden)]
    pub fn cff_font(&self) -> Option<&'a CffFontRef<'a>> {
        match self.base {
            Base::Cff(cff, _) => Some(cff),
            _ => None,
        }
    }

    /// Returns true if the font has no outlines from the requested sources.
    pub fn is_empty(&self) -> bool {
        matches!(self.base, Base::None)
    }

    /// Returns the number of glyphs in the font.
    ///
    /// Returns zero if the font has no outlines from the requested sources.
    pub fn len(&self) -> u32 {
        if self.is_empty() {
            0
        } else {
            self.font.num_glyphs()
        }
    }

    /// Returns the outline for `glyph`, or `None` if it has no outline or
    /// its data cannot be read.
    ///
    /// A composite that is malformed or nests past the recursion or edge
    /// limits returns an error when it is drawn.
    pub fn get(&self, glyph: GlyphId) -> Option<OutlineGlyph<'a>> {
        if glyph.to_u32() >= self.len() {
            return None;
        }
        match self.base {
            Base::None => None,
            Base::Glyf(context) => GlyfGlyph::new(context, glyph).map(OutlineGlyph::Glyf),
            Base::Cff(cff, source) => {
                let subfont_index = cff.subfont_index(glyph)?;
                let subfont = self.font.cff_subfont(source, subfont_index)?;
                Some(OutlineGlyph::Cff(CffGlyph {
                    cff,
                    coords: self.font.normalized_coords(),
                    glyph,
                    subfont_index,
                    subfont,
                    source,
                }))
            }
            Base::Type1(font) => Some(OutlineGlyph::Type1(Type1Glyph { font, glyph })),
        }
    }

    /// Returns an iterator over every glyph that has an outline, in glyph
    /// identifier order.
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, OutlineGlyph<'a>)> + 'a {
        let this = *self;
        (0..this.len())
            .map(GlyphId::new)
            .filter_map(move |glyph_id| this.get(glyph_id).map(|glyph| (glyph_id, glyph)))
    }
}

/// A glyph outline from a specific source.
///
/// Each variant contains the data needed to draw that source: a `glyf`
/// plan or a CFF subfont. To hint an outline, match on
/// the variant and use the source's own types.
#[non_exhaustive]
#[derive(Clone)]
pub enum OutlineGlyph<'a> {
    /// A TrueType outline.
    Glyf(GlyfGlyph<'a>),
    /// A `CFF` or `CFF2` charstring.
    Cff(CffGlyph<'a>),
    /// A Type 1 charstring.
    Type1(Type1Glyph<'a>),
}

impl OutlineGlyph<'_> {
    /// Returns the glyph identifier.
    pub fn glyph(&self) -> GlyphId {
        match self {
            Self::Glyf(glyph) => glyph.glyph(),
            Self::Cff(glyph) => glyph.glyph(),
            Self::Type1(glyph) => glyph.glyph(),
        }
    }

    /// Returns the table that provides the outline.
    pub fn source(&self) -> OutlineSource {
        match self {
            Self::Glyf(_) => OutlineSource::Glyf,
            Self::Cff(glyph) => glyph.source(),
            Self::Type1(_) => OutlineSource::Type1,
        }
    }

    /// Returns the size in bytes of the working memory needed by
    /// [`draw`](Self::draw).
    ///
    /// This is zero for charstrings.
    pub fn required_memory_size(&self) -> usize {
        match self {
            Self::Glyf(glyph) => glyph.required_memory_size(),
            Self::Cff(_) | Self::Type1(_) => 0,
        }
    }

    /// Draws the unhinted outline into `sink` with the given options.
    ///
    /// Supplied `memory` must hold at least
    /// [`required_memory_size`](Self::required_memory_size) bytes, or the draw
    /// fails with [`Error::InsufficientMemory`].
    ///
    /// Returns any metrics provided by the outline.
    pub fn draw(
        &self,
        options: &DrawOptions,
        memory: DrawMemory<'_>,
        sink: &mut dyn PathSink,
    ) -> Result<ComputedMetrics, Error> {
        self.draw_dyn(options, Precision::Fixed, memory.into_buffer(), sink)
    }

    /// Implements [`draw`](Self::draw) with the given arithmetic precision.
    ///
    /// Takes a trait object so the sources are compiled once rather than
    /// once per sink type.
    fn draw_dyn(
        &self,
        options: &DrawOptions,
        precision: Precision,
        memory: Option<&mut [u8]>,
        sink: &mut dyn PathSink,
    ) -> Result<ComputedMetrics, Error> {
        match self {
            Self::Cff(glyph) => glyph.draw(options, sink),
            Self::Type1(glyph) => glyph.draw(options, sink),
            Self::Glyf(glyph) => with_memory(
                memory,
                || glyph.required_memory_size(),
                |buf| glyph.draw(options, precision, buf, sink),
            ),
        }
    }

    /// Returns the size in bytes of the working memory needed by
    /// [`draw_unscaled`](Self::draw_unscaled).
    pub fn required_memory_size_unscaled(&self) -> usize {
        match self {
            Self::Glyf(glyph) => glyph.required_memory_size_unscaled(),
            Self::Cff(_) | Self::Type1(_) => 0,
        }
    }

    /// Sends the outline's points to `sink` in font units.
    ///
    /// A `glyf` outline provides its points as stored, with variations
    /// applied. Other sources are drawn as a path in font units and converted
    /// to points. When a contour closes at its start point, that point is not
    /// repeated.
    ///
    /// `memory` works as in [`draw`](Self::draw), with a size given by
    /// [`required_memory_size_unscaled`](Self::required_memory_size_unscaled).
    pub fn draw_unscaled(
        &self,
        memory: DrawMemory<'_>,
        sink: &mut dyn UnscaledOutlineSink,
    ) -> Result<ComputedMetrics, Error> {
        self.draw_unscaled_dyn(memory.into_buffer(), sink)
    }

    fn draw_unscaled_dyn(
        &self,
        memory: Option<&mut [u8]>,
        sink: &mut dyn UnscaledOutlineSink,
    ) -> Result<ComputedMetrics, Error> {
        let options = DrawOptions::default();
        match self {
            Self::Glyf(glyph) => match memory {
                Some(buf) => glyph.draw_unscaled(buf, sink),
                None => with_scratch(glyph.required_memory_size_unscaled(), |buf| {
                    glyph.draw_unscaled(buf, sink)
                }),
            },
            Self::Cff(glyph) => {
                let mut pen = PointsFromPath::new(sink);
                let metrics = glyph.draw(&options, &mut pen)?;
                pen.finish();
                Ok(metrics)
            }
            Self::Type1(glyph) => {
                let mut pen = PointsFromPath::new(sink);
                let metrics = glyph.draw(&options, &mut pen)?;
                pen.finish();
                Ok(metrics)
            }
        }
    }

    /// Returns the extents of the outline in font units.
    ///
    /// The extents are computed from the control points, so a curve's
    /// control points are included rather than the curve itself. An empty
    /// glyph has zero extents.
    pub fn extents(&self) -> Result<GlyphExtents<f32>, Error> {
        Ok(self
            .control_box()?
            .map(GlyphExtents::from)
            .unwrap_or_default())
    }

    /// Returns the bounding box of the control points.
    ///
    /// This is used for clips that intersect boxes.
    pub(crate) fn control_box(&self) -> Result<Option<BoundingBox<f32>>, Error> {
        let mut pen = ControlBoundsSink::new();
        // HarfBuzz keeps every fraction of a varied point.
        self.draw_dyn(&DrawOptions::default(), Precision::Float, None, &mut pen)?;
        Ok(pen.bounding_box())
    }
}

/// Runs `draw` with the caller's `memory`, or with scratch memory of the
/// size `size` returns when the caller has none.
fn with_memory<R>(
    memory: Option<&mut [u8]>,
    size: impl FnOnce() -> usize,
    mut draw: impl FnMut(&mut [u8]) -> R,
) -> R {
    match memory {
        Some(buf) => draw(buf),
        None => with_scratch(size(), draw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::glyph::outline::{NullSink, PathElement, SvgSink},
        FontRef, TableProvider,
    };
    use alloc::vec::Vec;

    fn font(data: &[u8]) -> Font {
        Font::new(data.to_vec(), 0).unwrap()
    }

    #[test]
    fn glyf_font_draws_from_glyf() {
        let font = font(font_test_data::VAZIRMATN_VAR);
        let glyphs = font.outline_glyphs();
        assert_eq!(glyphs.source(), Some(OutlineSource::Glyf));
        let glyph = glyphs.get(GlyphId::new(1)).unwrap();
        assert!(matches!(glyph, OutlineGlyph::Glyf(_)));
        let mut pen = SvgSink::new();
        let metrics = glyph
            .draw(&DrawOptions::default(), DrawMemory::Scratch, &mut pen)
            .unwrap();
        assert!(!pen.as_ref().is_empty());
        assert!(metrics.h_advance.is_some());
    }

    #[test]
    fn unscaled_fixed_and_float_agree_on_a_static_font() {
        let font = font(font_test_data::VAZIRMATN_VAR);
        let glyphs = font.outline_glyphs();
        for (glyph_id, glyph) in glyphs.iter() {
            let mut fixed = Vec::<PathElement>::new();
            glyph
                .draw(&DrawOptions::default(), DrawMemory::Scratch, &mut fixed)
                .unwrap();
            let mut float = Vec::<PathElement>::new();
            glyph
                .draw_dyn(&DrawOptions::default(), Precision::Float, None, &mut float)
                .unwrap();
            assert_eq!(fixed, float, "glyph {glyph_id}");
        }
    }

    #[test]
    fn caller_memory_too_small_is_refused() {
        let font = font(font_test_data::VAZIRMATN_VAR);
        let glyph = font.outline_glyphs().get(GlyphId::new(1)).unwrap();
        let options = DrawOptions::default();
        let needed = glyph.required_memory_size();
        assert!(needed > 0);
        let mut small = alloc::vec![0u8; needed / 2];
        let mut pen = NullSink;
        assert!(matches!(
            glyph.draw(&options, DrawMemory::Buffer(&mut small), &mut pen),
            Err(Error::InsufficientMemory)
        ));
        let mut enough = alloc::vec![0u8; needed];
        glyph
            .draw(&options, DrawMemory::Buffer(&mut enough), &mut pen)
            .unwrap();
    }

    #[test]
    fn cff_font_draws_from_cff() {
        let font = font(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED);
        let glyphs = font.outline_glyphs();
        assert_eq!(glyphs.source(), Some(OutlineSource::Cff));
        let glyph = glyphs.get(GlyphId::new(1)).unwrap();
        let OutlineGlyph::Cff(cff) = &glyph else {
            panic!("expected a charstring");
        };
        assert_eq!(cff.source(), OutlineSource::Cff);
        assert_eq!(glyph.required_memory_size(), 0);
        let mut pen = SvgSink::new();
        let metrics = glyph
            .draw(&DrawOptions::default(), DrawMemory::Scratch, &mut pen)
            .unwrap();
        assert!(!pen.as_ref().is_empty());
        // A CFF charstring states its width.
        assert!(metrics.h_advance.is_some());
    }

    // A standalone CFF font draws from its charstrings as an SFNT font draws
    // from its CFF table, and measures the same.
    #[test]
    fn standalone_cff_draws_like_its_table() {
        let sfnt = font(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED);
        let raw = FontRef::new(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED)
            .unwrap()
            .cff()
            .unwrap()
            .offset_data()
            .as_bytes()
            .to_vec();
        let bare = Font::new(raw, 0).unwrap();
        let (theirs, ours) = (sfnt.outline_glyphs(), bare.outline_glyphs());
        assert_eq!(ours.source(), Some(OutlineSource::Cff));
        assert!(ours.cff_font().is_some());
        assert_eq!(ours.len(), theirs.len());
        // An order without CFF finds nothing to draw.
        assert!(bare
            .outline_glyphs_with(GlyphSourceOrder::new(&[GlyphSource::Glyf]))
            .is_empty());
        let (their_metrics, our_metrics) = (sfnt.glyph_metrics(), bare.glyph_metrics());
        let mut drawn = 0;
        for (glyph, outline) in theirs.iter() {
            let mine = ours.get(glyph).unwrap();
            assert_eq!(mine.source(), OutlineSource::Cff);
            for ppem in [None, Some(16.0)] {
                let options = DrawOptions::default().with_ppem(ppem);
                let (mut expected, mut actual) = (SvgSink::new(), SvgSink::new());
                let want = outline
                    .draw(&options, DrawMemory::Scratch, &mut expected)
                    .unwrap();
                let got = mine
                    .draw(&options, DrawMemory::Scratch, &mut actual)
                    .unwrap();
                assert_eq!(
                    actual.as_ref(),
                    expected.as_ref(),
                    "glyph {glyph} at {ppem:?}"
                );
                assert_eq!(got, want, "glyph {glyph} at {ppem:?}");
            }
            assert_eq!(
                mine.extents().unwrap(),
                outline.extents().unwrap(),
                "glyph {glyph}"
            );
            assert_eq!(
                our_metrics.extents_exact(glyph),
                their_metrics.extents_exact(glyph),
                "glyph {glyph}"
            );
            drawn += 1;
        }
        assert!(drawn > 0);
    }

    #[test]
    fn order_without_the_fonts_format_is_empty() {
        let font = font(font_test_data::VAZIRMATN_VAR);
        let glyphs = font.outline_glyphs_with(GlyphSourceOrder::new(&[GlyphSource::Cff]));
        assert!(glyphs.is_empty());
        assert_eq!(glyphs.len(), 0);
        assert!(glyphs.get(GlyphId::new(1)).is_none());
        assert_eq!(glyphs.iter().count(), 0);
    }

    #[test]
    fn extents_of_a_glyf_glyph_match_its_header() {
        let font = font(font_test_data::VAZIRMATN_VAR);
        let (glyf, loca) = font.selected_glyf_loca().unwrap();
        let glyphs = font.outline_glyphs();
        let mut checked = 0;
        for (gid, glyph) in glyphs.iter() {
            let Some(header) = loca.get_glyf(gid, glyf).unwrap() else {
                continue;
            };
            if header.number_of_contours() == 0 {
                continue;
            }
            let extents = glyph.extents().unwrap();
            assert_eq!(extents.x_bearing, header.x_min() as f32);
            assert_eq!(extents.y_bearing, header.y_max() as f32);
            assert_eq!(extents.width, (header.x_max() - header.x_min()) as f32);
            assert_eq!(extents.height, (header.y_max() - header.y_min()) as f32);
            checked += 1;
        }
        assert!(checked > 0);
    }

    #[test]
    fn glyph_past_the_font_is_none() {
        let font = font(font_test_data::VAZIRMATN_VAR);
        let glyphs = font.outline_glyphs();
        assert!(glyphs.get(GlyphId::new(glyphs.len())).is_none());
        let font = font_ref_glyph_count(font_test_data::VAZIRMATN_VAR);
        assert_eq!(glyphs.len(), font);
    }

    fn font_ref_glyph_count(data: &[u8]) -> u32 {
        FontRef::new(data).unwrap().maxp().unwrap().num_glyphs() as u32
    }
}
