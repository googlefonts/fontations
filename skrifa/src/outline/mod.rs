//! Loading, scaling and hinting of glyph outlines.

// Temporary until new scaler API is done.
#![allow(dead_code)]

mod cff;
mod error;
mod glyf;
mod scaler;

use read_fonts::types::GlyphId;
pub use read_fonts::types::Pen;

pub use error::{Error, Result};
pub use scaler::{Scaler, ScalerBuilder, ScalerMetrics};

use super::{
    font::UniqueId,
    instance::{NormalizedCoord, Size},
    setting::VariationSetting,
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
            OutlineKind::Glyf(outline) => outline.has_overlaps,
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
            OutlineKind::Glyf(outline) => Some(outline.has_hinting),
            _ => None,
        }
    }

    /// Returns the size (in bytes) of the temporary memory required to load
    /// this outline.
    pub fn required_memory_size(&self) -> usize {
        match &self.kind {
            OutlineKind::Glyf(outline) => outline.required_buffer_size(),
            _ => 0,
        }
    }
}

#[derive(Clone)]
enum OutlineKind<'a> {
    Glyf(glyf::ScalerGlyph<'a>),
    // Subfont index
    Cff(u32),
}

/// Collection of scalable glyph outlines.
#[derive(Clone)]
pub struct OutlineCollection<'a> {
    kind: OutlineCollectionKind<'a>,
}

impl<'a> OutlineCollection<'a> {
    pub fn get(&self, glyph_id: GlyphId) -> Option<Outline<'a>> {
        match &self.kind {
            OutlineCollectionKind::None => None,
            OutlineCollectionKind::Glyf(glyf) => {
                
            }
            OutlineCollectionKind::Cff(cff) => {

            }
        }
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
#[cfg(feature = "hinting")]
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

/// Context for scaling glyphs.
///
/// This type contains temporary memory buffers and various internal caches to
/// accelerate the glyph scaling process.
///
/// See the [module level documentation](crate::scale#it-all-starts-with-a-context)
/// for more detail.
#[derive(Clone, Default, Debug)]
pub struct Context {
    /// Memory buffer for TrueType scaling buffers.
    outline_memory: Vec<u8>,
    /// Storage for normalized variation coordinates.
    coords: Vec<NormalizedCoord>,
    /// Storage for variation settings.
    variations: Vec<VariationSetting>,
}

impl Context {
    /// Creates a new glyph scaling context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a builder for configuring a glyph scaler.
    pub fn new_scaler(&mut self) -> ScalerBuilder {
        ScalerBuilder::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, Size};
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
        let mut cx = Context::new();
        let mut path = scaler_test::Path::default();
        let mut scaler = cx.new_scaler().build(&font);
        let glyph_count = font.maxp().unwrap().num_glyphs();
        // GID 2 is a composite glyph with the overlap bit on a component
        // GID 3 is a simple glyph with the overlap bit on the first flag
        let expected_gids_with_overlap = vec![2, 3];
        assert_eq!(
            expected_gids_with_overlap,
            (0..glyph_count)
                .filter(|gid| scaler
                    .outline(GlyphId::new(*gid), &mut path)
                    .unwrap()
                    .has_overlaps)
                .collect::<Vec<_>>()
        );
    }

    fn compare_glyphs(font_data: &[u8], expected_outlines: &str) {
        let font = FontRef::new(font_data).unwrap();
        let outlines = scaler_test::parse_glyph_outlines(expected_outlines);
        let mut cx = Context::new();
        let mut path = scaler_test::Path::default();
        for expected_outline in &outlines {
            if expected_outline.size == 0.0 && !expected_outline.coords.is_empty() {
                continue;
            }
            path.elements.clear();
            let mut scaler = cx
                .new_scaler()
                .size(Size::new(expected_outline.size))
                .normalized_coords(&expected_outline.coords)
                .build(&font);
            scaler
                .outline(expected_outline.glyph_id, &mut path)
                .unwrap();
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
