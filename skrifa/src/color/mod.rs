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
    pub xx: f32,
    pub yx: f32,
    pub xy: f32,
    pub yy: f32,
    pub dx: f32,
    pub dy: f32,
}

/// Reference to a paint graph that represents a color glyph outline.
#[derive(Clone)]
pub struct ColorOutline<'a> {
    colr: colr::Colr<'a>,
    kind: ColorOutlineKind<'a>,
}

impl<'a> ColorOutline<'a> {
    /// Returns the version of the color table from which this outline was
    /// selected.
    pub fn version(&self) -> u32 {
        match &self.kind {
            ColorOutlineKind::V0(..) => 0,
            ColorOutlineKind::V1(..) => 1,
        }
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
    colr: Option<colr::Colr<'a>>,
}

impl<'a> ColorOutlineCollection<'a> {
    /// Creates a new color outline collection for the given font.
    pub fn new(font: &impl TableProvider<'a>) -> Self {
        let colr = font.colr().ok();
        Self { colr }
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
        Some(ColorOutline { colr, kind })
    }

    /// Returns an iterator over all of the color outlines in the
    /// collection.
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, ColorOutline<'a>)> + 'a + Clone {
        let copy = self.clone();
        let max_glyph = copy
            .colr
            .as_ref()
            .map(|colr| {
                let max_v0 = if let Some(Ok(recs)) = colr.base_glyph_records() {
                    recs.last()
                        .map(|rec| rec.glyph_id().to_u16())
                        .unwrap_or_default()
                } else {
                    0
                };
                let max_v1 = if let Some(Ok(list)) = colr.base_glyph_list() {
                    list.base_glyph_paint_records()
                        .last()
                        .map(|rec| rec.glyph_id().to_u16())
                        .unwrap_or_default()
                } else {
                    0
                };
                max_v0.max(max_v1)
            })
            .unwrap_or_default();
        (0..=max_glyph).filter_map(move |gid| {
            let gid = GlyphId::new(gid);
            copy.get(gid).map(|outline| (gid, outline))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::MetadataProvider;
    use read_fonts::{types::GlyphId, FontRef};

    #[test]
    fn colr_outline_iter_and_version() {
        let font = FontRef::new(font_test_data::COLRV0V1_VARIABLE).unwrap();
        let outlines = font.color_outlines();
        // This font contains one COLRv0 glyph:
        // <GlyphID id="166" name="colored_circles_v0"/>
        let colrv0_outlines = [GlyphId::new(166)];
        for (gid, outline) in outlines.iter() {
            let expected_version = if colrv0_outlines.contains(&gid) { 0 } else { 1 };
            assert_eq!(outline.version(), expected_version);
        }
        // <!-- BaseGlyphRecordCount=1 -->
        // <!-- BaseGlyphCount=157 -->
        assert_eq!(outlines.iter().count(), 158);
    }
}
