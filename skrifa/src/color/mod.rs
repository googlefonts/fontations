//! Color outline support.

// Remove me when code is filled in
#![allow(unused_variables, dead_code)]

mod instance;

use core::ops::Range;

use crate::{instance::LocationRef, metrics::BoundingBox};
use read_fonts::{tables::colr, types::GlyphId, ReadError, TableProvider};

use instance::{resolve_paint, PaintId};

/// Interface for receiving a sequence of commands that represent a flattened
/// paint graph in a color outline.
// Placeholder
pub trait ColorPainter {}

/// Affine transformation matrix.
#[derive(Copy, Clone, Debug)]
pub struct Transform {
    xx: f32,
    yx: f32,
    xy: f32,
    yy: f32,
    dx: f32,
    dy: f32,
}

/// Reference to a paint graph that represents a color glyph outline.
#[derive(Clone)]
pub struct ColorOutline<'a> {
    colr: colr::Colr<'a>,
    glyph_id: GlyphId,
    kind: ColorOutlineKind<'a>,
}

impl<'a> ColorOutline<'a> {
    /// Returns the glyph identifier that was used to retrieve this outline.
    pub fn glyph_id(&self) -> GlyphId {
        self.glyph_id
    }

    /// Evaluates the paint graph at the specified location in variation space
    /// and returns the bounding box for the full scene.
    ///
    /// The `glyph_bounds` closure will be invoked for each clip node in the
    /// graph and should return the bounding box for the given glyph, location
    /// and transform.
    pub fn bounding_box(
        &self,
        location: impl Into<LocationRef<'a>>,
        glyph_bounds: impl FnMut(GlyphId, &LocationRef, Transform) -> Option<BoundingBox>,
    ) -> Option<BoundingBox> {
        None
    }

    /// Evaluates the paint graph at the specified location in variation space
    /// and emits the results to the given painter.
    pub fn paint(
        &self,
        location: impl Into<LocationRef<'a>>,
        painter: &mut impl ColorPainter,
    ) -> Result<(), ReadError> {
        let instance = instance::ColrInstance::new(self.colr.clone(), location.into().coords());
        match &self.kind {
            ColorOutlineKind::V0(layer_range) => {
                for layer_ix in layer_range.clone() {
                    let (glyph_id, palette_ix) = instance.v0_layer(layer_ix)?;
                }
            }
            ColorOutlineKind::V1(paint, paint_id) => {
                let paint = resolve_paint(&instance, paint)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
enum ColorOutlineKind<'a> {
    V0(Range<usize>),
    V1(colr::Paint<'a>, PaintId),
}

/// Collection of color outlines.
#[derive(Clone)]
pub struct ColorOutlineCollection<'a> {
    glyph_count: u16,
    colr: Option<colr::Colr<'a>>,
}

impl<'a> ColorOutlineCollection<'a> {
    /// Creates a new color outline collection for the given font.
    pub fn new(font: &impl TableProvider<'a>) -> Self {
        let glyph_count = font
            .maxp()
            .map(|maxp| maxp.num_glyphs())
            .unwrap_or_default();
        let colr = font.colr().ok();
        Self { glyph_count, colr }
    }

    /// Returns the color outline for the given glyph identifier.
    pub fn get(&self, glyph_id: GlyphId) -> Option<ColorOutline<'a>> {
        let colr = self.colr.clone()?;
        let kind = if let Ok(Some((paint, paint_id))) = colr.v1_base_glyph(glyph_id) {
            ColorOutlineKind::V1(paint, paint_id)
        } else {
            let layer_range = colr.v0_base_glyph(glyph_id).ok()??;
            ColorOutlineKind::V0(layer_range)
        };
        Some(ColorOutline {
            colr,
            glyph_id,
            kind,
        })
    }

    /// Returns an iterator over all of the color outlines in the
    /// collection.
    pub fn iter(&self) -> impl Iterator<Item = ColorOutline<'a>> + 'a + Clone {
        let copy = self.clone();
        (0..self.glyph_count).filter_map(move |gid| copy.get(GlyphId::new(gid)))
    }
}
