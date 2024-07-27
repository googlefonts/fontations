//! Segments and edges for one dimension of an outline.

use super::outline::{Direction, Orientation, Point};
use crate::collections::SmallVec;

/// Maximum number of segments and edges stored inline.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L306>
const MAX_INLINE_SEGMENTS: usize = 18;
const MAX_INLINE_EDGES: usize = 12;

/// Either horizontal or vertical.
///
/// A type alias because it's used as an index.
pub type Dimension = usize;

/// Segments and edges for one dimension of an outline.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L309>
#[derive(Clone, Default, Debug)]
pub struct Axis {
    /// Either horizontal or vertical.
    pub dim: Dimension,
    /// Depends on dimension and outline orientation.
    pub major_dir: Direction,
    /// Collection of segments for the axis.
    pub segments: SmallVec<Segment, MAX_INLINE_SEGMENTS>,
}

impl Axis {
    /// X coordinates, i.e. vertical segments and edges.
    pub const HORIZONTAL: Dimension = 0;
    /// Y coordinates, i.e. horizontal segments and edges.
    pub const VERTICAL: Dimension = 1;
}

impl Axis {
    pub fn new(dim: Dimension, orientation: Option<Orientation>) -> Self {
        let mut axis = Self::default();
        axis.reset(dim, orientation);
        axis
    }

    pub fn reset(&mut self, dim: Dimension, orientation: Option<Orientation>) {
        self.dim = dim;
        if dim == Self::HORIZONTAL {
            self.major_dir = Direction::Up;
        } else {
            self.major_dir = Direction::Left;
        }
        if orientation == Some(Orientation::Clockwise) {
            if dim == Self::HORIZONTAL {
                self.major_dir = Direction::Down;
            } else {
                self.major_dir = Direction::Right;
            }
        }
        self.segments.clear();
    }
}

/// Sequence of points with a single dominant direction.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L262>
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Segment {
    /// Flags describing the properties of the segment.
    pub flags: u8,
    /// Dominant direction of the segment.
    pub dir: Direction,
    /// Position of the segment.
    pub pos: i16,
    /// Deviation from segment position.
    pub delta: i16,
    /// Minimum coordinate of the segment.
    pub min_coord: i16,
    /// Maximum coordinate of the segment.
    pub max_coord: i16,
    /// Hinted segment height.
    pub height: i16,
    /// Used during stem matching.
    pub score: i32,
    /// Index of best candidate for a stem link.
    pub link_ix: Option<u16>,
    /// Index of best candidate for a serif link.
    pub serif_ix: Option<u16>,
    /// Index of first point in the outline.
    pub first_ix: u16,
    /// Index of last point in the outline.
    pub last_ix: u16,
}

/// Segment flags.
///
/// Note: these are the same as edge flags.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L227>
impl Segment {
    pub const NORMAL: u8 = 0;
    pub const ROUND: u8 = 1;
    pub const SERIF: u8 = 2;
    pub const DONE: u8 = 4;
    pub const NEUTRAL: u8 = 8;
}

impl Segment {
    pub fn first(&self) -> usize {
        self.first_ix as usize
    }

    pub fn first_point<'a>(&self, points: &'a [Point]) -> &'a Point {
        &points[self.first()]
    }

    pub fn first_point_mut<'a>(&self, points: &'a mut [Point]) -> &'a mut Point {
        &mut points[self.first()]
    }

    pub fn last(&self) -> usize {
        self.last_ix as usize
    }

    pub fn last_point<'a>(&self, points: &'a [Point]) -> &'a Point {
        &points[self.last()]
    }

    pub fn last_point_mut<'a>(&self, points: &'a mut [Point]) -> &'a mut Point {
        &mut points[self.last()]
    }
}
