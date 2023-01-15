/*!
TrueType outlines loaded from the `glyf` table.

*/

mod outline;
mod scaler;

pub use outline::Outline;
pub use scaler::Scaler;

pub use read_fonts::tables::glyf::Point;

use read_fonts::types::F26Dot6;

/// Context for loading for TrueType glyphs.
#[derive(Clone, Debug)]
pub struct Context {
    /// Unscaled points.
    unscaled: Vec<Point<i32>>,
    /// Original scaled points.
    original: Vec<Point<F26Dot6>>,
    /// Storage for variation deltas.
    deltas: Vec<Point<i32>>,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    /// Creates a new context.
    pub fn new() -> Self {
        Self {
            unscaled: vec![],
            original: vec![],
            deltas: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, Outline, Scaler};
    use crate::{font::*, GlyphId};

    use read_fonts::test_data::test_fonts;
    use read_fonts::types::F26Dot6;

    #[test]
    fn vazirmatin_var() {
        let font = FontRef::new(test_fonts::VAZIRMATN_VAR).unwrap();
        let outlines = crate::test::parse_glyph_outlines(test_fonts::VAZIRMATN_VAR_GLYPHS);
        let mut cx = Context::new();
        let mut outline = Outline::new();
        for expected_outline in &outlines {
            #[cfg(feature = "hinting")]
            let mut scaler = Scaler::new(&mut cx, &font, None, expected_outline.size, None, &[]).unwrap();
            #[cfg(not(feature = "hinting"))]
            let mut scaler = Scaler::new(&mut cx, &font, None, expected_outline.size, &[]).unwrap();
            scaler
                .load(expected_outline.glyph_id, &mut outline)
                .unwrap();
            assert_eq!(&outline.points, &expected_outline.points);
            assert_eq!(&outline.contours, &expected_outline.contours);
            assert_eq!(&outline.tags, &expected_outline.tags);
        }
    }
}
