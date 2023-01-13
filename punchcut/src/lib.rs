/*!
Glyph loading.
*/

#![forbid(unsafe_code)]
// TODO: this is temporary-- remove when hinting is added.
#![allow(dead_code, unused_imports, unused_variables)]

mod error;
mod scaler;
mod sink;

pub mod source;

/// Representations of fonts and font collections.
pub mod font {
    pub use read_fonts::{types::Tag, CollectionRef, FileRef, FontRef, TableProvider};
}

use font::Tag;
use source::glyf;

use core::str::FromStr;

pub use error::{Error, Result};
pub use scaler::{Scaler, ScalerBuilder};
pub use sink::PathSink;

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
#[derive(Default)]
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
    use super::{font::*, Context, GlyphId, PathSink, Scaler};

    #[derive(Copy, Clone, PartialEq, Debug)]
    enum PathElement {
        MoveTo([f32; 2]),
        LineTo([f32; 2]),
        QuadTo([f32; 4]),
        CurveTo([f32; 6]),
        Close,
    }

    use PathElement::*;

    impl PathSink for Vec<PathElement> {
        fn move_to(&mut self, x: f32, y: f32) {
            self.push(PathElement::MoveTo([x, y]));
        }

        fn line_to(&mut self, x: f32, y: f32) {
            self.push(PathElement::LineTo([x, y]));
        }

        fn quad_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
            self.push(PathElement::QuadTo([x0, y0, x1, y1]))
        }

        fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
            self.push(PathElement::CurveTo([x0, y0, x1, y1, x2, y2]))
        }

        fn close(&mut self) {
            self.push(PathElement::Close)
        }
    }

    fn test_glyph(font: &FontRef, gid: GlyphId, ppem: f32, expected_elements: &[PathElement]) {
        let mut cx = Context::new();
        let mut scaler = cx.new_scaler().size(ppem).build(font);
        let mut elements: Vec<PathElement> = vec![];
        scaler.outline(gid, &mut elements).unwrap();
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
                MoveTo([281.0, 1536.0]),
                LineTo([474.0, 1242.0]),
                LineTo([315.0, 1242.0]),
                LineTo([57.0, 1536.0]),
                Close,
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
                MoveTo([2.203125, 12.0]),
                LineTo([3.703125, 9.703125]),
                LineTo([2.46875, 9.703125]),
                LineTo([0.453125, 12.0]),
                Close,
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
                MoveTo([6.859375, 37.5]),
                LineTo([11.578125, 30.328125]),
                LineTo([7.6875, 30.328125]),
                LineTo([1.390625, 37.5]),
                Close,
            ],
        );
    }
}
