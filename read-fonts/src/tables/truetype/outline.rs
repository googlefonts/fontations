//! TrueType outline types.

use std::mem::size_of;

use crate::{
    tables::glyf::{to_path, PointFlags, ToPathError},
    types::{F26Dot6, Fixed, Pen, Point},
};

/// Memory requirements and metadata for scaling a TrueType outline.
#[derive(Copy, Clone, Default, Debug)]
pub struct OutlineInfo {
    /// Sum of the point counts of all simple glyphs in an outline.
    pub points: usize,
    /// Sum of the contour counts of all simple glyphs in an outline.
    pub contours: usize,
    /// Maximum number of points in a single simple glyph.
    pub max_simple_points: usize,
    /// "Other" points are the unscaled or original scaled points.
    ///
    /// The size of these buffer is the same and this value tracks the size
    /// for one (not both) of the buffers. This is the maximum of
    /// `max_simple_points` and the total number of points for all component
    /// glyphs in a single composite glyph.
    pub max_other_points: usize,
    /// Maximum size of the component delta stack.
    ///
    /// For composite glyphs in variable fonts, delta values are computed
    /// for each component. This tracks the maximum stack depth necessary
    /// to store those values during processing.
    pub max_component_delta_stack: usize,
    /// True if any component of a glyph has bytecode instructions.
    pub has_hinting: bool,
    /// True if the glyph requires variation delta processing.
    pub has_variations: bool,
}

impl OutlineInfo {
    /// Returns the minimum size in bytes required to scale an outline based
    /// on the computed sizes.
    pub fn required_buffer_size(&self) -> usize {
        let mut size = 0;
        // Scaled, unscaled and (for hinting) original scaled points
        size += self.points * size_of::<Point<F26Dot6>>();
        // Unscaled and (if hinted) original scaled points
        size +=
            self.max_other_points * size_of::<Point<i32>>() * if self.has_hinting { 2 } else { 1 };
        // Contour end points
        size += self.contours * size_of::<u16>();
        // Point flags
        size += self.points * size_of::<PointFlags>();
        if self.has_variations {
            // Interpolation buffer for delta IUP
            size += self.max_simple_points * size_of::<Point<Fixed>>();
            // Delta buffer for points
            size += self.max_simple_points * size_of::<Point<Fixed>>();
            // Delta buffer for composite components
            size += self.max_component_delta_stack * size_of::<Point<Fixed>>();
        }
        if size != 0 {
            // If we're given a buffer that is not aligned, we'll need to
            // adjust, so add our maximum alignment requirement in bytes.
            size += std::mem::align_of::<i32>();
        }
        size
    }
}

#[derive(Debug)]
pub struct Outline<'a> {
    pub points: &'a mut [Point<F26Dot6>],
    pub flags: &'a mut [PointFlags],
    pub contours: &'a mut [u16],
}

impl<'a> Outline<'a> {
    pub fn to_path(&self, pen: &mut impl Pen) -> Result<(), ToPathError> {
        to_path(self.points, self.flags, self.contours, pen)
    }
}

/// Outline data that is passed to the hinter.
pub struct HintOutline<'a> {
    pub unscaled: &'a mut [Point<i32>],
    pub scaled: &'a mut [Point<F26Dot6>],
    pub original_scaled: &'a mut [Point<F26Dot6>],
    pub flags: &'a mut [PointFlags],
    pub contours: &'a [u16],
    pub phantom: &'a mut [Point<F26Dot6>],
    pub bytecode: &'a [u8],
    pub is_composite: bool,
}
