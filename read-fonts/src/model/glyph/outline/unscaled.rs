//! Outlines in font units, as points rather than path commands.
//!
//! An autohinter needs outline points in the form the `glyf` table stores
//! them: whole font units, marked on-curve or off-curve, and grouped by
//! contour. [`UnscaledOutlineSink`] receives these points.

use crate::tables::glyf::PointFlags;

/// Receives the points of an outline in font units, one contour at a time.
pub trait UnscaledOutlineSink {
    /// Begins a contour.
    fn start_contour(&mut self);

    /// Adds a point to the open contour.
    ///
    /// `flags` indicate whether the point is on the curve or is an off-curve
    /// control point of a quadratic or cubic curve. The interpreter marker
    /// flags are always clear.
    fn point(&mut self, x: i32, y: i32, flags: PointFlags);

    /// Ends the open contour.
    fn end_contour(&mut self);
}

impl<S: UnscaledOutlineSink + ?Sized> UnscaledOutlineSink for &mut S {
    fn start_contour(&mut self) {
        (**self).start_contour()
    }

    fn point(&mut self, x: i32, y: i32, flags: PointFlags) {
        (**self).point(x, y, flags)
    }

    fn end_contour(&mut self) {
        (**self).end_contour()
    }
}
