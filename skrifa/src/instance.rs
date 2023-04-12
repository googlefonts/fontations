//! Helpers for selecting a font size and location in variation space.

/// Type for a normalized variation coordinate.
pub type NormalizedCoord = read_fonts::types::F2Dot14;

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
    /// per em value which can be retrieved from the [Metrics](crate::metrics::Metrics)
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

/// Reference to an ordered sequence of normalized variation coordinates.
///
/// This type represents a position in the variation space where each
/// coordinate corresponds to an axis (in the same order as the `fvar` table)
/// and is a normalized value in the range `[-1..1]`.
///
/// See [Coordinate Scales and Normalization](https://learn.microsoft.com/en-us/typography/opentype/spec/otvaroverview#coordinate-scales-and-normalization)
/// for further details.
///
/// If the array is larger in length than the number of axes, extraneous
/// values are ignored. If it is smaller, unrepresented axes are assumed to be
/// at their default positions (i.e. 0).
///
/// A value of this type constructed with `default()` represents the default
/// position for each axis.
///
/// Normalized coordinates are ignored for non-variable fonts.
#[derive(Copy, Clone, Default, Debug)]
pub struct LocationRef<'a>(&'a [NormalizedCoord]);

impl<'a> LocationRef<'a> {
    /// Creates a new sequence of normalized coordinates from the given array.
    pub fn new(coords: &'a [NormalizedCoord]) -> Self {
        Self(coords)
    }

    /// Returns the underlying array of normalized coordinates.
    pub fn coords(&self) -> &'a [NormalizedCoord] {
        self.0
    }
}

impl<'a> From<&'a [NormalizedCoord]> for LocationRef<'a> {
    fn from(value: &'a [NormalizedCoord]) -> Self {
        Self(value)
    }
}

impl<'a> IntoIterator for LocationRef<'a> {
    type IntoIter = core::slice::Iter<'a, NormalizedCoord>;
    type Item = &'a NormalizedCoord;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'_ LocationRef<'a> {
    type IntoIter = core::slice::Iter<'a, NormalizedCoord>;
    type Item = &'a NormalizedCoord;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
