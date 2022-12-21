use super::Point;

/// TrueType outline.
#[derive(Default, PartialEq, Eq, Debug)]
pub struct Outline {
    /// Set of points that define the shape of the outline.
    pub points: Vec<Point>,
    /// Set of tags (one per point).
    pub tags: Vec<u8>,
    /// Index of the end points for each contour in the outline.
    pub contours: Vec<u16>,
    /// True if the scaler applied a scale, in which case the points are in
    /// 26.6 fixed point format. Otherwise, they are in integral font units.
    pub is_scaled: bool,
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
        self.is_scaled = false;
    }
}
