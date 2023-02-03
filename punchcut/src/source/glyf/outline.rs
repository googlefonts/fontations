use super::Point;
use crate::PathSink;

use read_fonts::{tables::glyf::ToPathError, types::F26Dot6};

/// TrueType outline.
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct Outline {
    /// Set of points that define the shape of the outline.
    pub points: Vec<Point<F26Dot6>>,
    /// Set of tags (one per point).
    pub tags: Vec<u8>,
    /// Index of the end points for each contour in the outline.
    pub contours: Vec<u16>,
}

impl Outline {
    /// Creates a new empty outline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties the outline.
    pub fn clear(&mut self) {
        self.points.clear();
        self.tags.clear();
        self.contours.clear();
    }

    /// Converts the outline to a sequence of path commands and invokes the callback for
    /// each on the given sink.
    pub fn to_path(&self, sink: &mut impl PathSink<f32>) -> Result<(), ToPathError> {
        read_fonts::tables::glyf::to_path(&self.points, &self.tags, &self.contours, sink)
    }
}
