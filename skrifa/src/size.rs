//! Strongly typed font size representation.

/// Font size in pixels per em units.
///
/// Sizes in this crate are represented as a ratio of pixels to the size of
/// the em square defined by the font. This is equivalent to the `px` unit
/// in CSS (assuming a DPI scale factor of 1.0).
///
/// To retrieve metrics and outlines in font units, use the [unscaled](Self::unscaled)
/// construtor on this type.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct Size(f32);

impl Size {
    /// Creates a new font size from the given value in pixels per em units.
    ///
    /// Providing a value `<= 0.0` is equivalent to creating an unscaled size
    /// and will result in metrics and outlines generated in font units.
    pub fn new(ppem: f32) -> Self {
        Self(ppem)
    }

    /// Creates a new font size for generating unscaled metrics or outlines in
    /// font units.
    pub fn unscaled() -> Self {
        Self(0.0)
    }

    /// Returns the raw size in pixels per em units.
    ///
    /// Results in `None` if the size is unscaled.
    pub fn ppem(self) -> Option<f32> {
        (self.0 > 0.0).then_some(self.0)
    }

    /// Computes a linear scale factor for this font size and the given units
    /// per em value which can be retrieved from the [Metrics](crate::meta::metrics::Metrics)
    /// type or from the [head](read_fonts::tables::head::Head) table.
    ///
    /// Returns 1.0 for an unscaled size or when `units_per_em` is 0.
    pub fn linear_scale(self, units_per_em: u16) -> f32 {
        if self.0 > 0.0 && units_per_em != 0 {
            self.0 / units_per_em as f32
        } else {
            1.0
        }
    }
}
