//! Pen implementations based on <https://github.com/fonttools/fonttools/tree/main/Lib/fontTools/pens>

use font_types::{Pen, PenCommand};
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
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.map_point(x, y);
        self.inner_pen.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.map_point(x, y);
        self.inner_pen.line_to(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (cx0, cy0) = self.map_point(cx0, cy0);
        let (x, y) = self.map_point(x, y);
        self.inner_pen.quad_to(cx0, cy0, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (cx0, cy0) = self.map_point(cx0, cy0);
        let (cx1, cy1) = self.map_point(cx1, cy1);
        let (x, y) = self.map_point(x, y);
        self.inner_pen.curve_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.inner_pen.close();
    }
}

pub struct BezPathPen {
    path: BezPath,
}

fn as_kurbo_point(x: f32, y: f32) -> Point {
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

    pub fn into_inner(self) -> BezPath {
        self.path
    }
}

impl Default for BezPathPen {
    fn default() -> Self {
        Self::new()
    }
}

impl Pen for BezPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(as_kurbo_point(x, y))
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(as_kurbo_point(x, y))
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path
            .quad_to(as_kurbo_point(cx0, cy0), as_kurbo_point(x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            as_kurbo_point(cx0, cy0),
            as_kurbo_point(cx1, cy1),
            as_kurbo_point(x, y),
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

#[derive(Debug)]
pub enum ContourReversalError {
    InvalidFirstCommand(PenCommand),
    SubpathDoesNotStartWithMoveTo,
}

/// Buffers commands until a close is seen, then plays in reverse on inner pen.
///
/// Reverses the winding direction of the contour. Keeps the first point unchanged.
///
/// <https://github.com/fonttools/fonttools/blob/78e10d8b42095b709cd4125e592d914d3ed1558e/Lib/fontTools/pens/reverseContourPen.py#L8>
pub struct ReverseContourPen<'a, T: Pen> {
    inner_pen: &'a mut T,
    pending: Vec<PenCommand>,
}

/// Reverse the commands in a path.
///
///  FontTools version in
/// <https://github.com/fonttools/fonttools/blob/78e10d8b42095b709cd4125e592d914d3ed1558e/Lib/fontTools/pens/reverseContourPen.py#L25>
fn flush_subpath<T: Pen>(commands: &[PenCommand], pen: &mut T) -> Result<(), ContourReversalError> {
    if commands.is_empty() {
        return Ok(());
    }

    let mut commands = commands;
    let mut reversed = Vec::new();

    // subpath must start with a move, and by definition it can't have any other move
    let PenCommand::MoveTo { x, y } = commands[0] else {
        return Err(ContourReversalError::SubpathDoesNotStartWithMoveTo);
    };
    let (start_x, start_y) = (x, y);

    // When reversed, the move is to the end point of the last command
    // in a typical [move, ..., close] structure, end point == start point
    let (end_x, end_y) = commands
        .last()
        .unwrap()
        .end_point()
        .unwrap_or((start_x, start_y));
    reversed.push(PenCommand::MoveTo { x: end_x, y: end_y });
    commands = &commands[1..];

    // Reverse the commands between move (if any) and final close (if any)
    for (idx, cmd) in commands.iter().enumerate().rev() {
        let (end_x, end_y) = if idx > 0 {
            commands[idx - 1].end_point().unwrap_or((start_x, start_y))
        } else {
            (start_x, start_y)
        };
        reversed.push(match *cmd {
            PenCommand::MoveTo { .. } => {
                panic!("Subpath should have 0 or 1 moves, and it should already have been removed")
            }
            PenCommand::LineTo { .. } => PenCommand::LineTo { x: end_x, y: end_y },
            PenCommand::QuadTo { cx0, cy0, .. } => PenCommand::QuadTo {
                cx0,
                cy0,
                x: end_x,
                y: end_y,
            },
            PenCommand::CurveTo {
                cx0, cy0, cx1, cy1, ..
            } => PenCommand::CurveTo {
                cx0: cx1,
                cy0: cy1,
                cx1: cx0,
                cy1: cy0,
                x: end_x,
                y: end_y,
            },
            // Close is a line from (end_x,end_y)=>(start_x, start_y) so reversed it's a line to end x/y
            PenCommand::Close => PenCommand::LineTo { x: end_x, y: end_y },
        });
    }

    // a closing line to start is a Z
    if let Some(PenCommand::LineTo { x, y }) = reversed.last() {
        if (start_x, start_y) == (*x, *y) {
            *reversed.last_mut().unwrap() = PenCommand::Close;
        }
    }

    // send to inner
    reversed.into_iter().for_each(|c| c.apply_to(pen));

    Ok(())
}

impl<'a, T: Pen> ReverseContourPen<'a, T> {
    pub fn new(inner_pen: &'a mut T) -> ReverseContourPen<T> {
        ReverseContourPen {
            inner_pen,
            pending: Vec::new(),
        }
    }

    /// Flush buffer into inner, reversing the winding order.
    ///
    /// If start == end, as is typically in a [move, ..., close] structure, start point is unchanged.
    ///
    /// Requires an explicit call to afford the client the opportunity to receive errors.
    pub fn flush(&mut self) -> Result<(), ContourReversalError> {
        // Process subpath by subpath
        let mut subpath_starts: Vec<usize> = self
            .pending
            .iter()
            .enumerate()
            .filter_map(|(idx, cmd)| {
                match cmd {
                    // Move starts a new subpath
                    // However, since we will split on idx ignore 0
                    PenCommand::MoveTo { .. } => Some(idx),
                    _ => None,
                }
            })
            .collect();
        subpath_starts.push(self.pending.len());
        for win in subpath_starts.windows(2) {
            flush_subpath(&self.pending[win[0]..win[1]], self.inner_pen)?;
        }

        self.pending.clear();

        Ok(())
    }
}

impl<'a, T: Pen> Pen for ReverseContourPen<'a, T> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pending.push(PenCommand::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.pending.push(PenCommand::LineTo { x, y });
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.pending.push(PenCommand::QuadTo { cx0, cy0, x, y });
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.pending.push(PenCommand::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.pending.push(PenCommand::Close);
    }
}

#[cfg(test)]
mod tests {
    use font_types::Pen;
    use kurbo::Affine;

    use super::{BezPathPen, ReverseContourPen, TransformPen};

    fn draw_open_test_shape(pen: &mut impl Pen) {
        pen.move_to(10.0, 10.0);
        pen.quad_to(40.0, 40.0, 60.0, 10.0);
        pen.line_to(100.0, 10.0);
        pen.curve_to(125.0, 10.0, 150.0, 50.0, 125.0, 60.0)
    }

    fn draw_closed_triangle(pen: &mut impl Pen) {
        pen.move_to(100.0, 100.0);
        pen.line_to(150.0, 200.0);
        pen.line_to(50.0, 200.0);
        pen.close();
    }

    fn draw_closed_test_shape(pen: &mut impl Pen) {
        pen.move_to(125.0, 100.0);
        pen.quad_to(200.0, 150.0, 175.0, 300.0);
        pen.curve_to(150.0, 150.0, 50.0, 150.0, 25.0, 300.0);
        pen.quad_to(0.0, 150.0, 75.0, 100.0);
        pen.line_to(100.0, 50.0);
        pen.close();
    }

    #[test]
    fn double_double_toil_and_trouble() {
        let mut bez = BezPathPen::new();
        bez.move_to(1.0, 1.0);
        bez.line_to(2.0, 2.0);

        let mut double = TransformPen::new(&mut bez, Affine::scale(2.0));
        double.move_to(1.0, 1.0);
        double.line_to(2.0, 2.0);

        // We should see the move/line passed through double is doubled
        assert_eq!("M1 1L2 2M2 2L4 4", bez.into_inner().to_svg());
    }

    #[test]
    fn reverse_unclosed() {
        let mut bez = BezPathPen::new();
        draw_open_test_shape(&mut bez);
        assert_eq!(
            "M10 10Q40 40 60 10L100 10C125 10 150 50 125 60",
            bez.into_inner().to_svg()
        );

        let mut bez = BezPathPen::new();
        let mut rev = ReverseContourPen::new(&mut bez);
        draw_open_test_shape(&mut rev);
        rev.flush().unwrap();
        assert_eq!(
            "M125 60C150 50 125 10 100 10L60 10Q40 40 10 10",
            bez.into_inner().to_svg()
        );
    }

    #[test]
    fn reverse_closed_triangle() {
        let mut bez = BezPathPen::new();
        draw_closed_triangle(&mut bez);
        assert_eq!("M100 100L150 200L50 200Z", bez.into_inner().to_svg());

        let mut bez = BezPathPen::new();
        let mut rev = ReverseContourPen::new(&mut bez);
        draw_closed_triangle(&mut rev);
        rev.flush().unwrap();
        assert_eq!("M100 100L50 200L150 200Z", bez.into_inner().to_svg());
    }

    #[test]
    fn reverse_closed_shape() {
        let mut bez = BezPathPen::new();
        draw_closed_test_shape(&mut bez);
        assert_eq!(
            "M125 100Q200 150 175 300C150 150 50 150 25 300Q0 150 75 100L100 50Z",
            bez.into_inner().to_svg()
        );

        let mut bez = BezPathPen::new();
        let mut rev = ReverseContourPen::new(&mut bez);
        draw_closed_test_shape(&mut rev);
        rev.flush().unwrap();
        assert_eq!(
            "M125 100L100 50L75 100Q0 150 25 300C50 150 150 150 175 300Q200 150 125 100",
            bez.into_inner().to_svg()
        );
    }
}
