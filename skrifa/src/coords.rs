//! Representation of a set of normalized coordinates.

use super::NormalizedCoord;

/// Ordered sequence of normalized variation coordinates in design space.
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
pub struct NormalizedCoords<'a>(&'a [NormalizedCoord]);

impl<'a> NormalizedCoords<'a> {
    /// Creates a new sequence of normalized coordinates from the given array.
    pub fn new(coords: &'a [NormalizedCoord]) -> Self {
        Self(coords)
    }

    /// Returns the underlying array of normalized coordinates.
    pub fn inner(&self) -> &'a [NormalizedCoord] {
        self.0
    }
}

impl<'a> From<&'a [NormalizedCoord]> for NormalizedCoords<'a> {
    fn from(value: &'a [NormalizedCoord]) -> Self {
        Self(value)
    }
}

impl<'a> IntoIterator for NormalizedCoords<'a> {
    type IntoIter = core::slice::Iter<'a, NormalizedCoord>;
    type Item = &'a NormalizedCoord;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'_ NormalizedCoords<'a> {
    type IntoIter = core::slice::Iter<'a, NormalizedCoord>;
    type Item = &'a NormalizedCoord;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
