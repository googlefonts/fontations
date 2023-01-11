use peniko::kurbo::{BezPath, PathEl};

/// Interface for path output.
pub trait PathSink {
    /// Move command.
    fn move_to(&mut self, x: f32, y: f32);

    /// Line segment command.
    fn line_to(&mut self, x: f32, y: f32);

    /// Quadratic bezier segment command.
    fn quad_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32);

    /// Cubic bezier segment command.
    fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32);

    /// Close subpath command.
    fn close(&mut self);
}

impl PathSink for BezPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.move_to((x as f64, y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.line_to((x as f64, y as f64));
    }

    fn quad_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.quad_to((x0 as f64, y0 as f64), (x1 as f64, y1 as f64));
    }

    fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.curve_to(
            (x0 as f64, y0 as f64),
            (x1 as f64, y1 as f64),
            (x2 as f64, y2 as f64),
        );
    }

    fn close(&mut self) {
        self.close_path();
    }
}

impl PathSink for Vec<PathEl> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.push(PathEl::MoveTo((x as f64, y as f64).into()))
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(PathEl::LineTo((x as f64, y as f64).into()))
    }

    fn quad_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.push(PathEl::QuadTo(
            (x0 as f64, y0 as f64).into(),
            (x1 as f64, y1 as f64).into(),
        ))
    }

    fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.push(PathEl::CurveTo(
            (x0 as f64, y0 as f64).into(),
            (x1 as f64, y1 as f64).into(),
            (x2 as f64, y2 as f64).into(),
        ));
    }

    fn close(&mut self) {
        self.push(PathEl::ClosePath);
    }
}
