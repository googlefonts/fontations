//! Loading, scaling and hinting of glyph outlines.

// Temporary until new scaler API is done.
#![allow(dead_code)]

mod cff;
mod error;
mod glyf;
mod scaler;

pub use read_fonts::types::Pen;
use read_fonts::{types::GlyphId, TableProvider};

pub use error::{Error, Result};
pub use scaler::{Scaler, ScalerMetrics};

use super::{
    instance::{LocationRef, NormalizedCoord, Size},
    GLYF_COMPOSITE_RECURSION_LIMIT,
};

#[derive(Clone)]
pub struct Outline<'a> {
    kind: OutlineKind<'a>,
}

impl<'a> Outline<'a> {
    /// Returns true if the outline may contain overlapping contours or
    /// components.
    pub fn has_overlaps(&self) -> bool {
        match &self.kind {
            OutlineKind::Glyf(_, outline) => outline.has_overlaps,
            _ => false,
        }
    }

    /// Returns a value indicating whether the outline has hinting
    /// instructions.
    ///
    /// For CFF outlines, returns `None` since this is unknown prior
    /// to loading the outline.
    pub fn has_hinting(&self) -> Option<bool> {
        match &self.kind {
            OutlineKind::Glyf(_, outline) => Some(outline.has_hinting),
            _ => None,
        }
    }

    /// Returns the size (in bytes) of the temporary memory required to load
    /// this outline.
    pub fn required_memory_size(&self, with_hinting: bool) -> usize {
        match &self.kind {
            OutlineKind::Glyf(_, outline) => outline.required_buffer_size(with_hinting),
            _ => 0,
        }
    }

    pub fn scale(
        &self,
        scaler: &Scaler,
        memory: Option<&mut [u8]>,
        pen: &mut impl Pen,
    ) -> Option<ScalerMetrics> {
        scaler.scale(self, memory, pen)
    }
}

#[derive(Clone)]
enum OutlineKind<'a> {
    Glyf(glyf::Scaler<'a>, glyf::ScalerGlyph<'a>),
    // Second field is subfont index
    Cff(cff::Scaler<'a>, GlyphId, u32),
}

/// Collection of scalable glyph outlines.
#[derive(Clone)]
pub struct OutlineCollection<'a> {
    kind: OutlineCollectionKind<'a>,
}

impl<'a> OutlineCollection<'a> {
    pub fn new(font: &impl TableProvider<'a>) -> Self {
        let kind = if let Some(glyf) = glyf::Scaler::new(font) {
            OutlineCollectionKind::Glyf(glyf)
        } else if let Ok(cff) = cff::Scaler::new(font) {
            OutlineCollectionKind::Cff(cff)
        } else {
            OutlineCollectionKind::None
        };
        Self { kind }
    }

    pub fn get(&self, glyph_id: GlyphId) -> Option<Outline<'a>> {
        match &self.kind {
            OutlineCollectionKind::None => None,
            OutlineCollectionKind::Glyf(glyf) => Some(Outline {
                kind: OutlineKind::Glyf(glyf.clone(), glyf.glyph(glyph_id, true).ok()?),
            }),
            OutlineCollectionKind::Cff(cff) => Some(Outline {
                kind: OutlineKind::Cff(cff.clone(), glyph_id, cff.subfont_index(glyph_id)),
            }),
        }
    }

    pub fn scaler_instance(
        &self,
        size: Size,
        location: impl Into<LocationRef<'a>>,
        hinting: Option<Hinting>,
    ) -> Option<Scaler> {
        Scaler::new(self, size, location.into(), hinting)
    }
}

#[derive(Clone)]
enum OutlineCollectionKind<'a> {
    None,
    Glyf(glyf::Scaler<'a>),
    Cff(cff::Scaler<'a>),
}

/// Modes for hinting.
///
/// Only the `glyf` source supports all hinting modes.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum Hinting {
    /// "Full" hinting mode. May generate rough outlines and poor horizontal
    /// spacing.
    Full,
    /// Light hinting mode. This prevents most movement in the horizontal
    /// direction with the exception of a per-font backward compatibility
    /// opt in.
    Light,
    /// Same as light, but with additional support for RGB subpixel rendering.
    LightSubpixel,
    /// Same as light subpixel, but always prevents adjustment in the
    /// horizontal direction. This is the default mode.
    #[default]
    VerticalSubpixel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetadataProvider;
    use read_fonts::{scaler_test, types::GlyphId, FontRef, TableProvider};

    #[test]
    fn vazirmatin_var() {
        compare_glyphs(
            font_test_data::VAZIRMATN_VAR,
            font_test_data::VAZIRMATN_VAR_GLYPHS,
        );
    }

    #[test]
    fn cantarell_vf() {
        compare_glyphs(
            font_test_data::CANTARELL_VF_TRIMMED,
            font_test_data::CANTARELL_VF_TRIMMED_GLYPHS,
        );
    }

    #[test]
    fn noto_serif_display() {
        compare_glyphs(
            font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
            font_test_data::NOTO_SERIF_DISPLAY_TRIMMED_GLYPHS,
        );
    }

    #[test]
    fn overlap_flags() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let outlines = font.outlines();
        let scaler = outlines
            .scaler_instance(Size::unscaled(), LocationRef::default(), None)
            .unwrap();
        let mut path = scaler_test::Path::default();
        let glyph_count = font.maxp().unwrap().num_glyphs();
        // GID 2 is a composite glyph with the overlap bit on a component
        // GID 3 is a simple glyph with the overlap bit on the first flag
        let expected_gids_with_overlap = vec![2, 3];
        assert_eq!(
            expected_gids_with_overlap,
            (0..glyph_count)
                .filter(|gid| outlines
                    .get(GlyphId::new(*gid))
                    .unwrap()
                    .scale(&scaler, None, &mut path)
                    .unwrap()
                    .has_overlaps)
                .collect::<Vec<_>>()
        );
    }

    fn compare_glyphs(font_data: &[u8], expected_outlines: &str) {
        let font = FontRef::new(font_data).unwrap();
        let expected_outlines = scaler_test::parse_glyph_outlines(expected_outlines);
        let mut path = scaler_test::Path::default();
        for expected_outline in &expected_outlines {
            if expected_outline.size == 0.0 && !expected_outline.coords.is_empty() {
                continue;
            }
            path.elements.clear();
            let scaler = font
                .outlines()
                .scaler_instance(
                    Size::new(expected_outline.size),
                    expected_outline.coords.as_slice(),
                    None,
                )
                .unwrap();
            let outline = font.outlines().get(expected_outline.glyph_id).unwrap();
            outline.scale(&scaler, None, &mut path).unwrap();
            if path.elements != expected_outline.path {
                panic!(
                    "mismatch in glyph path for id {} (size: {}, coords: {:?}): path: {:?} expected_path: {:?}",
                    expected_outline.glyph_id,
                    expected_outline.size,
                    expected_outline.coords,
                    &path.elements,
                    &expected_outline.path
                );
            }
        }
    }
}
