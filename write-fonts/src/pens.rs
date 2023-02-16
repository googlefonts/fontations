//! Pen implementations based on <https://github.com/fonttools/fonttools/tree/main/Lib/fontTools/pens>

use std::mem;

use font_types::Pen;
use kurbo::{Affine, BezPath, Point};

/// A pen that transforms params using [kurbo::Affine].
pub struct TransformPen<'a, T: Pen> {
    inner_pen: &'a mut T,
    transform: Affine,
}

impl<'a, T: Pen> TransformPen<'a, T> {
    pub fn new(inner_pen: &'a mut T, transform: Affine) -> TransformPen<'a, T> {
        TransformPen {
            inner_pen,
            transform,
        }
    }

    fn map_point(&self, x: f32, y: f32) -> (f32, f32) {
        let pt = self.transform
            * Point {
                x: x as f64,
                y: y as f64,
            };
        (pt.x as f32, pt.y as f32)
    }
}

impl<'a, T: Pen> Pen for TransformPen<'a, T> {
    fn move_to(&mut self, mut x: f32, mut y: f32) {
        (x, y) = self.map_point(x, y);
        self.inner_pen.move_to(x, y);
    }

    fn line_to(&mut self, mut x: f32, mut y: f32) {
        (x, y) = self.map_point(x, y);
        self.inner_pen.line_to(x, y);
    }

    fn quad_to(&mut self, mut cx0: f32, mut cy0: f32, mut x: f32, mut y: f32) {
        (cx0, cy0) = self.map_point(cx0, cy0);
        (x, y) = self.map_point(x, y);
        self.inner_pen.quad_to(cx0, cy0, x, y);
    }

    fn curve_to(
        &mut self,
        mut cx0: f32,
        mut cy0: f32,
        mut cx1: f32,
        mut cy1: f32,
        mut x: f32,
        mut y: f32,
    ) {
        (cx0, cy0) = self.map_point(cx0, cy0);
        (cx1, cy1) = self.map_point(cx1, cy1);
        (x, y) = self.map_point(x, y);
        self.inner_pen.curve_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.inner_pen.close();
    }
}

pub struct BezPathPen {
    path: BezPath,
}

fn pt(x: f32, y: f32) -> Point {
    Point {
        x: x as f64,
        y: y as f64,
    }
}

impl BezPathPen {
    pub fn new() -> BezPathPen {
        BezPathPen {
            path: BezPath::new(),
        }
    }

    pub fn take_path(&mut self) -> BezPath {
        mem::replace(&mut self.path, BezPath::new())
    }
}

impl Default for BezPathPen {
    fn default() -> Self {
        Self::new()
    }
}

impl Pen for BezPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(pt(x, y))
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(pt(x, y))
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path.quad_to(pt(cx0, cy0), pt(x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(pt(cx0, cy0), pt(cx1, cy1), pt(x, y));
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

#[cfg(test)]
mod tests {
    use font_types::Pen;
    use kurbo::Affine;

    use super::{BezPathPen, TransformPen};

    #[test]
    fn double_double_toil_and_trouble() {
        let mut bez = BezPathPen::new();
        bez.move_to(1.0, 1.0);
        bez.line_to(2.0, 2.0);

        let mut double = TransformPen::new(&mut bez, Affine::scale(2.0));
        double.move_to(1.0, 1.0);
        double.line_to(2.0, 2.0);

        assert_eq!("M1 1L2 2M2 2L4 4", bez.take_path().to_svg());
    }
}
