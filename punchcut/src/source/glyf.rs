/*!
TrueType outlines loaded from the `glyf` table.

*/

mod math;
mod outline;
mod scaler;

pub use outline::Outline;
pub use scaler::Scaler;

pub use read_fonts::tables::glyf::Point;

/// Context for loading for TrueType glyphs.
pub struct Context {
    /// Unscaled points.
    unscaled: Vec<Point>,
    /// Original unscaled points.
    original: Vec<Point>,
    /// Storage for variation deltas.
    deltas: Vec<Point>,
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

    fn test_glyph(
        font: &FontRef,
        gid: GlyphId,
        ppem: f32,
        expected_contours: &[u16],
        expected_points: &[(i32, i32)],
        expected_tags: &[u8],
    ) {
        let mut cx = Context::new();
        let mut scaler = Scaler::new(&mut cx, font, None, ppem, None, &[]).unwrap();
        let mut outline = Outline::new();
        scaler.load(GlyphId::new(3), &mut outline).unwrap();
        let points = outline
            .points
            .iter()
            .map(|pt| (pt.x, pt.y))
            .collect::<Vec<_>>();
        assert_eq!(&outline.contours[..], expected_contours);
        assert_eq!(&points[..], expected_points);
        assert_eq!(&outline.tags[..], expected_tags);
    }

    #[test]
    fn unscaled() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        test_glyph(
            &font,
            GlyphId::new(3),
            0.0,
            &[3],
            // Unscaled points in font units
            &[(281, 1536), (474, 1242), (315, 1242), (57, 1536)],
            &[1, 1, 1, 1],
        );
    }

    #[test]
    fn scaled_16_ppem() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        test_glyph(
            &font,
            GlyphId::new(3),
            16.0,
            &[3],
            // Points scaled to 16ppem as computed by FreeType
            &[(141, 768), (237, 621), (158, 621), (29, 768)],
            &[1, 1, 1, 1],
        );
    }

    #[test]
    fn scaled_50_ppem() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        test_glyph(
            &font,
            GlyphId::new(3),
            50.0,
            &[3],
            // Points scaled to 50ppem as computed by FreeType
            &[(439, 2400), (741, 1941), (492, 1941), (89, 2400)],
            &[1, 1, 1, 1],
        );
    }
}
