/// Interface for accepting a sequence of path commands.
///
/// This is a general abstraction to unify ouput for processes that decode and/or
/// transform outlines.
///
/// The type parameter `T` represents the coordinate type.
pub trait PathSink<T> {
    /// Emit a command to begin a new subpath at (x, y).
    fn move_to(&mut self, x: T, y: T);

    /// Emit a line segment from the current point to (x, y).
    fn line_to(&mut self, x: T, y: T);

    /// Emit a quadratic bezier segment from the current point with a control
    /// point at (cx0, cy0) and ending at (x, y).
    fn quad_to(&mut self, cx0: T, cy0: T, x: T, y: T);

    /// Emit a cubic bezier segment from the current point with control
    /// points at (cx0, cy0) and (cx1, cy1) and ending at (x, y).
    fn curve_to(&mut self, cx0: T, cy0: T, cx1: T, cy1: T, x: T, y: T);

    /// Emit a command to close the current subpath.
    fn close(&mut self);
}
