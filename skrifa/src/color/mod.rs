//! Drawing color glyphs.
//!
//! # Examples
//! ## Retrieve the clip box of a COLRv1 glyph if it has one:
//!
//! ```
//! # use skrifa::{scale::*, instance::Size, color::{ColorPaintableType, ColorPainter}};
//! # use read_fonts::GlyphId;
//! # fn get_colr_bb(font: read_fonts::FontRef, color_painter_impl : &mut ColorPainter, glyph_id : GlyphId) {
//! match font.color_paintables()
//!       .get_type(glyph_id, ColorPaintableType::ColrV1)?
//!       .get_bounding_box(&[], size)
//!       .ok()?
//! {
//! Some(bounding_box) => {
//!     println!("Bounding box is {:?}", bounding_box);
//! }
//! None => {
//!     println!("Glyph has no clip box.")
//! }
//! }
//! # }
//! ```
//!
//! ## Paint a COLRv1 glyph given a font, and a glyph id and a [`ColorPainter`] implementation:
//! ```
//! # use skrifa::{scale::*, instance::Size, color::{ColorPaintableType, ColorPainter}};
//! # use read_fonts::GlyphId;
//! # fn paint_colr(font: read_fonts::FontRef, color_painter_impl : &mut ColorPainter, glyph_id : GlyphId) -> Result<(), ColorError> {
//! let color_paintable = font.color_paintables().get_type(glyph_id, ColorPaintableType::ColrV1)?;
//! color_paintable.paint(&[], color_painter_impl)?
//! # }
//! ```
//!
mod instance;
mod transform;
mod traversal;

#[cfg(test)]
mod traversal_tests;

use raw::tables::colr;
#[cfg(test)]
use serde::{Deserialize, Serialize};

use read_fonts::{
    tables::colr::{CompositeMode, Extend},
    types::{BoundingBox, GlyphId, Point},
    ReadError, TableProvider,
};

use std::{collections::HashSet, fmt::Debug, ops::Range};

use traversal::{get_clipbox_font_units, traverse_with_callbacks};

pub use transform::Transform;

use crate::prelude::LocationRef;

use self::instance::{resolve_paint, PaintId};

/// An error during drawing a COLR glyph. This covers inconsistencies
/// in the COLRv1 paint graph as well as downstream
/// parse errors from read-fonts.
#[derive(Debug, Clone)]
pub enum ColorError {
    ParseError(ReadError),
    NoColrV1GlyphForId(GlyphId),
    PaintCycleDetected,
}

impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ColorError::ParseError(read_error) => {
                write!(f, "Error parsing font data: {read_error}")
            }
            ColorError::NoColrV1GlyphForId(glyph_id) => {
                write!(f, "No COLRv1 glyph found for glyph id: {glyph_id}")
            }
            ColorError::PaintCycleDetected => write!(f, "Paint cycle detected in COLRv1 glyph."),
        }
    }
}

impl From<ReadError> for ColorError {
    fn from(value: ReadError) -> Self {
        ColorError::ParseError(value)
    }
}

#[derive(Clone, Debug, Default)]
// This repr(C) is required so that C-side FFI's
// are able to cast the ColorStop slice to a C-side array pointer.
#[repr(C)]
#[derive(PartialEq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
/// A color stop of a gradient. All gradient callbacks of [`ColorPainter`] normalize color stops to be in the range of 0
/// to 1. `palette_index` specifies a color from the CPAL table, to be multiplied with `alpha` before use.
pub struct ColorStop {
    pub offset: f32,
    pub palette_index: u16,
    pub alpha: f32,
}

// Design considerations for choosing a slice of ColorStops as `color_stop`
// type: In principle, a local `Vec<ColorStop>` allocation would not required if
// we're willing to walk the `ResolvedColorStop` iterator to find the minimum
// and maximum color stops.  Then we could scale the color stops based on the
// minimum and maximum. But performing the min/max search would require
// re-applying the deltas at least once, after which we would pass the scaled
// stops to client side and have the client sort the collected items once
// again. If we do want to pre-ort them, and still use use an
// `Iterator<Item=ColorStop>`` instead as the `color_stops` field, then we would
// need a Fontations-side allocations to sort, and an extra allocation on the
// client side to `.collect()` from the provided iterator before passing it to
// drawing API.
//
/// Used for encapsulating the different types of fills that may occur in a
/// COLRv1 glyph. The client receives the information about the fill type in the
/// [`fill``](ColorPainter::fill) callback of the [`ColorPainter`] trait.
#[derive(Debug, PartialEq)]
pub enum FillType<'a> {
    /// A solid fill with the color specified by `palette_index`. The respective
    /// color from the CPAL table then needs to be multiplied with `alpha`.
    Solid { palette_index: u16, alpha: f32 },
    /// A linear gradient, normalized from the P0, P1 and P2 representation in
    /// the COLRv1 table to a linear gradient between two points `p0` and
    /// `p1`. If there is only one color stop, the client should draw a solid
    /// fill with that color. The `color_stops` are normalized to the range from
    /// 0 to 1.
    LinearGradient {
        p0: Point<f32>,
        p1: Point<f32>,
        color_stops: &'a [ColorStop],
        extend: Extend,
    },
    /// A radial gradient, with color stops normalized to the range of 0 to
    /// 1. Caution: This normalization can mean that negative radii occur. It is
    /// the client's responsibility to truncate the color line at the 0
    /// position, interpolating between `r0` and `r1` and compute an
    /// interpolated color at that position.
    RadialGradient {
        c0: Point<f32>,
        r0: f32,
        c1: Point<f32>,
        r1: f32,
        color_stops: &'a [ColorStop],
        extend: Extend,
    },
    /// A sweep gradient, also called conical gradient. The color stops are
    /// normalized to the range from 0 to 1 and the returned angles are to be
    /// interpreted in _clockwise_ direction (swapped from the meaning in the
    /// font file).  The stop normalization may mean that the angles may be
    /// larger or smaller than the range of 0 to 360. Note that only the range
    /// from 0 to 360 degrees is to be drawn, see
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/colr#sweep-gradients>.
    SweepGradient {
        c0: Point<f32>,
        start_angle: f32,
        end_angle: f32,
        color_stops: &'a [ColorStop],
        extend: Extend,
    },
}

/// Result type used in [`draw_color_glyph`](ColorPainter::draw_color_glyph)
/// through which the client can signal whether a COLRv1 glyph referenced by
/// another COLRv1 glyph can be drawn from cache or whether the glyphs subgraph
/// should be traversed.
pub enum ColrGlyphDrawResult {
    ColrGlyphDrawn,
    ColrGlyphDrawingNotImpl,
}

/// A group of required callbacks to be provided by the client. Each callback is
/// executing a particular drawing or canvas transformation operation. The
/// trait's callback functions are invoked when [`paint`](ColorPaintable::paint) is
/// called with a [`ColorPainter`] trait object. The documentation for each
/// function describes what actions are to be executed using the client side 2D
/// graphics API, usually by performing some kind of canvas operation.
pub trait ColorPainter {
    /// Push the specified transform by concatenating it to the current
    /// transformation matrix.
    fn push_transform(&mut self, transform: Transform);
    /// Restore the transformation matrix to the state before the previous
    /// [`push_transform`](ColorPainter::push_transform) call.
    fn pop_transform(&mut self);

    /// Apply a clip path in the shape of glyph specified by `glyph_id`.
    fn push_clip_glyph(&mut self, glyph_id: GlyphId);
    /// Apply a clip rectangle specified by `clip_rect`.
    fn push_clip_box(&mut self, clip_box: BoundingBox<f32>);
    /// Restore the clip state to the state before a previous
    /// [`push_clip_glyph`](ColorPainter::push_clip_glyph) or
    /// [`push_clip_box`](ColorPainter::push_clip_box) call.
    fn pop_clip(&mut self);

    /// Fill the current clip area with the specified gradient fill.
    fn fill(&mut self, fill_type: FillType);

    /// Optionally implement this method: Draw an unscaled COLRv1 glyph given
    /// the current transformation matrix (as accumulated by
    /// [`push_transform`](ColorPainter::push_transform) calls).
    fn draw_color_glyph(&mut self, _glyph: GlyphId) -> Result<ColrGlyphDrawResult, ReadError> {
        Ok(ColrGlyphDrawResult::ColrGlyphDrawingNotImpl)
    }

    // TODO(drott): Add an optimized callback function combining clip, fill and transforms.

    /// Open a new layer, and merge the layer down using `composite_mode` when
    /// [`pop_layer`](ColorPainter::pop_layer) is called, signalling that this layer is done drawing.
    fn push_layer(&mut self, composite_mode: CompositeMode);
    fn pop_layer(&mut self);
}

/// Distinguishes available color glyph types.
pub enum ColorPaintableType {
    ColrV0,
    ColrV1,
}

/// A representation of a color glyph that can be painted through a sequence of [`ColorPainter`] callbacks.
#[derive(Clone)]
pub struct ColorPaintable<'a> {
    colr: colr::Colr<'a>,
    root_paint_ref: ColorPaintableRoot<'a>,
}

#[derive(Clone)]
enum ColorPaintableRoot<'a> {
    V0Range(Range<usize>),
    V1Paint(colr::Paint<'a>, PaintId, GlyphId, Result<u16, ReadError>),
}

impl<'a> ColorPaintable<'a> {
    /// Returns the version of the color table from which this outline was
    /// selected.
    pub fn paintable_type(&self) -> ColorPaintableType {
        match &self.root_paint_ref {
            ColorPaintableRoot::V0Range(_) => ColorPaintableType::ColrV0,
            ColorPaintableRoot::V1Paint(..) => ColorPaintableType::ColrV1,
        }
    }

    /// Returns the bounding box. For COLRv1 paintables, this is clipbox of the
    /// specified COLRv1 glyph, or `None` if there is
    /// none for the particular glyph.  The `size` argument can optionally be used
    /// to scale the bounding box to a particular font size. `location` allows
    /// specifycing a variation instance.
    pub fn get_bounding_box(
        &self,
        location: impl Into<LocationRef<'a>>,
        size: Option<f32>,
    ) -> Result<Option<BoundingBox<f32>>, ColorError> {
        let instance = instance::ColrInstance::new(self.colr.clone(), location.into().coords());

        match &self.root_paint_ref {
            ColorPaintableRoot::V1Paint(_paint, _paint_id, glyph_id, upem) => {
                let upem = (*upem).clone()?;
                let resolved_bounding_box = get_clipbox_font_units(&instance, *glyph_id)?;

                let scaled_clipbox = resolved_bounding_box.map(|bounding_box| {
                    let scale_factor = size.map(|size| size / upem as f32).unwrap_or(1.0);
                    BoundingBox {
                        x_min: bounding_box.x_min * scale_factor,
                        y_min: bounding_box.y_min * scale_factor,
                        x_max: bounding_box.x_max * scale_factor,
                        y_max: bounding_box.y_max * scale_factor,
                    }
                });
                Ok(scaled_clipbox)
            }
            _ => todo!(),
        }
    }

    /// Evaluates the paint graph at the specified location in variation space
    /// and emits the results to the given painter.
    ///
    ///
    /// For a COLRv1 glyph, traverses the COLRv1 paint graph and invokes drawing callbacks on a
    /// specified [`ColorPainter`] trait object.  The traversal operates in font
    /// units and will call `ColorPainter` methods with font unit values. This
    /// means, if you want to draw a COLRv1 glyph at a particular font size, the
    /// canvas needs to have a transformation matrix applied so that it scales down
    /// the drawing operations to `font_size / upem`.
    ///
    /// # Arguments
    ///
    /// * `glyph_id` the `GlyphId` to be drawn.
    /// * `location` coordinates for specifying a variation instance. This can be empty.
    /// * `painter` a client-provided [`ColorPainter`] implementation receiving drawing callbacks.
    ///
    pub fn paint(
        &self,
        location: impl Into<LocationRef<'a>>,
        painter: &mut impl ColorPainter,
    ) -> Result<(), ColorError> {
        let instance = instance::ColrInstance::new(self.colr.clone(), location.into().coords());
        match &self.root_paint_ref {
            ColorPaintableRoot::V1Paint(paint, paint_id, glyph_id, _) => {
                let clipbox = get_clipbox_font_units(&instance, *glyph_id)?;

                if let Some(rect) = clipbox {
                    painter.push_clip_box(rect);
                }

                let mut visited_set: HashSet<usize> = HashSet::new();
                visited_set.insert(*paint_id);
                traverse_with_callbacks(
                    &resolve_paint(&instance, paint)?,
                    &instance,
                    painter,
                    &mut visited_set,
                )?;

                if clipbox.is_some() {
                    painter.pop_clip();
                }
                Ok(())
            }
            _ => todo!(),
        }
    }
}

/// Collection of paintable color glyphs.
#[derive(Clone)]
pub struct ColorPaintableCollection<'a> {
    colr: Option<colr::Colr<'a>>,
    upem: Result<u16, ReadError>,
}

impl<'a> ColorPaintableCollection<'a> {
    /// Creates a new collection of paintable color glyphs for the given font.
    pub fn new(font: &impl TableProvider<'a>) -> Self {
        let colr = font.colr().ok();
        let upem = font.head().map(|h| h.units_per_em());

        Self { colr, upem }
    }

    /// Returns the paintable color glyph representation for the given glyph identifier.
    pub fn get_type(
        &self,
        glyph_id: GlyphId,
        paintable_type: ColorPaintableType,
    ) -> Option<ColorPaintable<'a>> {
        let colr = self.colr.clone()?;

        let root_paint_ref = match paintable_type {
            ColorPaintableType::ColrV0 => {
                let layer_range = colr.v0_base_glyph(glyph_id).ok()??;
                ColorPaintableRoot::V0Range(layer_range)
            }
            ColorPaintableType::ColrV1 => {
                let (paint, paint_id) = colr.v1_base_glyph(glyph_id).ok()??;
                ColorPaintableRoot::V1Paint(paint, paint_id, glyph_id, self.upem.clone())
            }
        };
        Some(ColorPaintable {
            colr,
            root_paint_ref,
        })
    }

    pub fn get_most_expressive(&self, glyph_id: GlyphId) -> Option<ColorPaintable<'a>> {
        self.get_type(glyph_id, ColorPaintableType::ColrV1)
            .or_else(|| self.get_type(glyph_id, ColorPaintableType::ColrV0))
    }
}

#[cfg(test)]
mod tests {

    use crate::{prelude::LocationRef, MetadataProvider};
    use read_fonts::{types::BoundingBox, FontRef};

    use super::{ColorPainter, CompositeMode, FillType, GlyphId, Transform};

    #[test]
    fn has_colrv1_glyph_test() {
        let colr_font = font_test_data::COLRV0V1_VARIABLE;
        let font = FontRef::new(colr_font).unwrap();
        let get_colrv1_paintable = |glyph_id| {
            font.color_paintables()
                .get_type(glyph_id, crate::color::ColorPaintableType::ColrV1)
        };

        assert!(get_colrv1_paintable(GlyphId::new(166)).is_none());
        assert!(get_colrv1_paintable(GlyphId::new(167)).is_some());
    }
    struct DummyColorPainter {}

    impl DummyColorPainter {
        pub fn new() -> Self {
            Self {}
        }
    }

    impl Default for DummyColorPainter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ColorPainter for DummyColorPainter {
        fn push_transform(&mut self, _transform: Transform) {}
        fn pop_transform(&mut self) {}
        fn push_clip_glyph(&mut self, _glyph: GlyphId) {}
        fn push_clip_box(&mut self, _clip_box: BoundingBox<f32>) {}
        fn pop_clip(&mut self) {}
        fn fill(&mut self, _brush: FillType) {}
        fn push_layer(&mut self, _composite_mode: CompositeMode) {}
        fn pop_layer(&mut self) {}
    }

    #[test]
    fn paintcolrglyph_cycle_test() {
        let colr_font = font_test_data::COLRV0V1_VARIABLE;
        let font = FontRef::new(colr_font).unwrap();
        let colrv1_paintable = font
            .color_paintables()
            .get_type(GlyphId::new(176), crate::color::ColorPaintableType::ColrV1);

        assert!(colrv1_paintable.is_some());
        let mut color_painter = DummyColorPainter::new();

        let result = colrv1_paintable
            .unwrap()
            .paint(LocationRef::default(), &mut color_painter);
        // Expected to fail with an error as the glyph contains a paint cycle.
        assert!(result.is_err());
    }
}
