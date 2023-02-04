/*!
Glyph loading.
*/

#![forbid(unsafe_code)]
// TODO: this is temporary-- remove when hinting is added.
#![allow(dead_code, unused_imports, unused_variables)]

mod error;
mod scaler;

#[cfg(test)]
mod test;

pub mod source;

/// Representations of fonts and font collections.
pub mod font {
    pub use read_fonts::{types::Tag, CollectionRef, FileRef, FontRef, TableProvider};
}

use font::Tag;
use source::glyf;

use core::str::FromStr;

pub use read_fonts::types::Pen;

pub use error::{Error, Result};
pub use scaler::{Scaler, ScalerBuilder};

/// Limit for recursion when loading TrueType composite glyphs.
const GLYF_COMPOSITE_RECURSION_LIMIT: usize = 32;

/// Modes for hinting.
///
/// Only the `glyf` source supports all hinting modes.
#[cfg(feature = "hinting")]
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum Hinting {
    /// "Full" hinting mode. May generate rough outlines and poor horizontal
    /// spacing.
    Full,
    /// Light hinting mode. This prevents most movement in the horizontal direction
    /// with the exception of a per-font backward compatibility opt in.
    Light,
    /// Same as light, but with additional support for RGB subpixel rendering.
    LightSubpixel,
    /// Same as light subpixel, but always prevents adjustment in the horizontal
    /// direction. This is the default mode.
    #[default]
    VerticalSubpixel,
}

/// Type for a normalized variation coordinate.
pub type NormalizedCoord = read_fonts::types::F2Dot14;

/// Type for a glyph identifier.
pub type GlyphId = read_fonts::types::GlyphId;

/// Setting for specifying a variation by tag and value.
#[derive(Copy, Clone, Debug)]
pub struct Variation {
    /// Tag for the variation.
    pub tag: Tag,
    /// Value for the variation.
    pub value: f32,
}

impl From<(Tag, f32)> for Variation {
    fn from(s: (Tag, f32)) -> Self {
        Self {
            tag: s.0,
            value: s.1,
        }
    }
}

impl From<(&str, f32)> for Variation {
    fn from(s: (&str, f32)) -> Self {
        Self {
            tag: Tag::from_str(s.0).unwrap_or_default(),
            value: s.1,
        }
    }
}

impl From<([u8; 4], f32)> for Variation {
    fn from(s: ([u8; 4], f32)) -> Self {
        Self {
            tag: Tag::new_checked(&s.0[..]).unwrap_or_default(),
            value: s.1,
        }
    }
}

/// Context for loading glyphs.
#[derive(Clone, Default, Debug)]
pub struct Context {
    /// Inner context for loading TrueType outlines.
    glyf: glyf::Context,
    /// Internal storage for TrueType outlines.
    glyf_outline: glyf::Outline,
    /// Storage for normalized variation coordinates.
    coords: Vec<NormalizedCoord>,
    /// Storage for variation settings.
    variations: Vec<Variation>,
}

impl Context {
    /// Creates a new glyph loading context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a builder for configuring a scaler.
    pub fn new_scaler(&mut self) -> ScalerBuilder {
        ScalerBuilder::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{font::*, Context, GlyphId, Pen, Scaler};
    use read_fonts::test_data::test_fonts;

    #[test]
    fn vazirmatin_var() {
        let font = FontRef::new(test_fonts::VAZIRMATN_VAR).unwrap();
        let outlines = crate::test::parse_glyph_outlines(test_fonts::VAZIRMATN_VAR_GLYPHS);
        let mut cx = Context::new();
        let mut path = crate::test::Path::default();
        for expected_outline in &outlines {
            path.0.clear();
            let mut scaler = cx
                .new_scaler()
                .size(expected_outline.size)
                .coords(&expected_outline.coords)
                .build(&font);
            scaler
                .outline(expected_outline.glyph_id, &mut path)
                .unwrap();
            if path.0 != expected_outline.path {
                panic!(
                    "mismatch in glyph path for id {} (size: {}, coords: {:?}): path: {:?} expected_path: {:?}",
                    expected_outline.glyph_id.to_u16(),
                    expected_outline.size,
                    expected_outline.coords,
                    &path.0,
                    &expected_outline.path
                );
            }
        }
    }
}
