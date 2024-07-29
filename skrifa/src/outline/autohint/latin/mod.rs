//! Latin writing system.

mod blues;
mod segments;

impl super::outline::Outline {
    /// All constants are defined based on a UPEM of 2048.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L34>
    fn latin_constant(&self, value: i32) -> i32 {
        value * self.units_per_em as i32 / 2048
    }
}
