/*!
Glyph loading.
*/

#![forbid(unsafe_code)]
// TODO: this is temporary-- remove when hinting is added.
#![allow(dead_code, unused_imports, unused_variables)]

/// Re-export of peniko crate.
pub use peniko;

mod error;
mod outline;
mod scaler;

pub mod source;

/// Representations of fonts and font collections.
pub mod font {
    pub use read_fonts::{types::Tag, CollectionRef, FileRef, FontRef, TableProvider};
}

use font::Tag;
use source::*;

use core::str::FromStr;

pub use error::*;
pub use outline::*;
pub use scaler::*;

/// Modes for hinting.
///
/// Only the `glyf` source supports all hinting modes.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
#[allow(dead_code)]
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

/// Context for loading glyphs from any available source.
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

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    /// Creates a new glyph loading context.
    pub fn new() -> Self {
        Self {
            glyf: glyf::Context::new(),
            glyf_outline: glyf::Outline::new(),
            coords: vec![],
            variations: vec![],
        }
    }

    /// Returns a builder for configuring a scaler.
    pub fn new_scaler(&mut self) -> ScalerBuilder {
        ScalerBuilder::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        font::*,
        peniko::kurbo::{
            PathEl::{self, *},
            Point,
        },
        Context, GlyphId, Scaler,
    };

    fn test_glyph(font: &FontRef, gid: GlyphId, ppem: f32, expected_elements: &[PathEl]) {
        let mut cx = Context::new();
        let mut scaler = cx.new_scaler().size(ppem).build(font);
        let outline = scaler.outline(gid).unwrap();
        let elements = outline.elements().collect::<Vec<_>>();
        assert_eq!(&elements[..], expected_elements);
    }

    #[test]
    fn unscaled() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        test_glyph(
            &font,
            GlyphId::new(3),
            0.0,
            // Path elements in unscaled font units
            &[
                MoveTo((281.0, 1536.0).into()),
                LineTo((474.0, 1242.0).into()),
                LineTo((315.0, 1242.0).into()),
                LineTo((57.0, 1536.0).into()),
                ClosePath,
            ],
        );
    }

    #[test]
    fn scaled_16_ppem() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        test_glyph(
            &font,
            GlyphId::new(3),
            16.0,
            // Path elements scaled to 16ppem as computed by FreeType
            &[
                MoveTo((2.203125, 12.0).into()),
                LineTo((3.703125, 9.703125).into()),
                LineTo((2.46875, 9.703125).into()),
                LineTo((0.453125, 12.0).into()),
                ClosePath,
            ],
        );
    }

    #[test]
    fn scaled_50_ppem() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        test_glyph(
            &font,
            GlyphId::new(3),
            50.0,
            // Path elements scaled to 50ppem as computed by FreeType
            &[
                MoveTo((6.859375, 37.5).into()),
                LineTo((11.578125, 30.328125).into()),
                LineTo((7.6875, 30.328125).into()),
                LineTo((1.390625, 37.5).into()),
                ClosePath,
            ],
        );
    }
}
