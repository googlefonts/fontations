/// Interface for accepting a sequence of path commands.
///
/// This is a general abstraction to unify ouput for processes that decode and/or
/// transform outlines.
///
/// /// AbstractPen in Python terms.
/// <https://github.com/fonttools/fonttools/blob/78e10d8b42095b709cd4125e592d914d3ed1558e/Lib/fontTools/pens/basePen.py#L54>
pub trait Pen {
    /// Emit a command to begin a new subpath at (x, y).
    fn move_to(&mut self, x: f32, y: f32);

    /// Emit a line segment from the current point to (x, y).
    fn line_to(&mut self, x: f32, y: f32);

    /// Emit a quadratic bezier segment from the current point with a control
    /// point at (cx0, cy0) and ending at (x, y).
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32);

    /// Emit a cubic bezier segment from the current point with control
    /// points at (cx0, cy0) and (cx1, cy1) and ending at (x, y).
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32);

    /// Emit a command to close the current subpath.
    fn close(&mut self);
}
