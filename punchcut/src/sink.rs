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
