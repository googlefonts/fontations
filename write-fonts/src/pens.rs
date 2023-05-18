//! Pen implementations based on <https://github.com/fonttools/fonttools/tree/main/Lib/fontTools/pens>

use font_types::{Pen, PenCommand};
use kurbo::{Affine, BezPath, PathEl, Point, Rect};

pub fn write_to_pen(path: &BezPath, pen: &mut impl Pen) {
    path.elements()
        .iter()
        .map(|e| to_pen_command(*e))
        .for_each(|c| c.apply_to(pen));
}

pub fn to_pen_command(el: PathEl) -> PenCommand {
    match el {
        PathEl::MoveTo(Point { x, y }) => PenCommand::MoveTo {
            x: x as f32,
            y: y as f32,
        },
        PathEl::LineTo(Point { x, y }) => PenCommand::LineTo {
            x: x as f32,
            y: y as f32,
        },
        PathEl::QuadTo(c0, Point { x, y }) => PenCommand::QuadTo {
            cx0: c0.x as f32,
            cy0: c0.y as f32,
            x: x as f32,
            y: y as f32,
        },
        PathEl::CurveTo(c0, c1, Point { x, y }) => PenCommand::CurveTo {
            cx0: c0.x as f32,
            cy0: c0.y as f32,
            cx1: c1.x as f32,
            cy1: c1.y as f32,
            x: x as f32,
            y: y as f32,
        },
        PathEl::ClosePath => PenCommand::Close,
    }
}

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
/// Reverses the winding direction of the contour. Keeps the first point unchanged. In FontTools terms
/// we implement only reversedContour(..., outputImpliedClosingLine=True) as this appears to be necessary
/// to ensure retention of interpolation.
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

/// Records commands as [PenCommand]
pub struct RecordingPen {
    commands: Vec<PenCommand>,
}

impl RecordingPen {
    pub fn new() -> RecordingPen {
        RecordingPen {
            commands: Vec::new(),
        }
    }

    pub fn commands(&self) -> &[PenCommand] {
        &self.commands
    }
}

impl Default for RecordingPen {
    fn default() -> Self {
        Self::new()
    }
}

impl Pen for RecordingPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(PenCommand::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(PenCommand::LineTo { x, y });
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.commands.push(PenCommand::QuadTo { cx0, cy0, x, y });
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.commands.push(PenCommand::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.commands.push(PenCommand::Close);
    }
}

/// Pen to calculate the "control bounds" of a shape. This is the
/// bounding box of all control points, so may be larger than the
/// actual bounding box if there are curves that don't have points
/// on their extremes.
///
/// <https://github.com/fonttools/fonttools/blob/main/Lib/fontTools/pens/boundsPen.py>
#[derive(Default)]
pub struct ControlBoundsPen {
    bounds: Option<Rect>,
}

impl ControlBoundsPen {
    pub fn new() -> Self {
        Self { bounds: None }
    }

    fn grow_to_include(&mut self, x: f32, y: f32) {
        let (x, y) = (x as f64, y as f64);
        self.bounds = Some(match self.bounds {
            Some(rect) => rect.union_pt((x, y).into()),
            None => Rect {
                x0: x,
                y0: y,
                x1: x,
                y1: y,
            },
        })
    }

    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }
}

impl Pen for ControlBoundsPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.grow_to_include(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.grow_to_include(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.grow_to_include(cx0, cy0);
        self.grow_to_include(x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.grow_to_include(cx0, cy0);
        self.grow_to_include(cx1, cy1);
        self.grow_to_include(x, y);
    }

    fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use font_types::{Pen, PenCommand};
    use kurbo::{Affine, BezPath, Rect, Shape};

    use super::{
        write_to_pen, BezPathPen, ControlBoundsPen, RecordingPen, ReverseContourPen, TransformPen,
    };

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
        assert_eq!("M1,1 L2,2 M2,2 L4,4", bez.into_inner().to_svg());
    }

    #[test]
    fn reverse_unclosed() {
        let mut bez = BezPathPen::new();
        draw_open_test_shape(&mut bez);
        assert_eq!(
            "M10,10 Q40,40 60,10 L100,10 C125,10 150,50 125,60",
            bez.into_inner().to_svg()
        );

        let mut bez = BezPathPen::new();
        let mut rev = ReverseContourPen::new(&mut bez);
        draw_open_test_shape(&mut rev);
        rev.flush().unwrap();
        assert_eq!(
            "M125,60 C150,50 125,10 100,10 L60,10 Q40,40 10,10",
            bez.into_inner().to_svg()
        );
    }

    #[test]
    fn reverse_closed_triangle() {
        let mut bez = BezPathPen::new();
        draw_closed_triangle(&mut bez);
        assert_eq!("M100,100 L150,200 L50,200 Z", bez.into_inner().to_svg());

        let mut bez = BezPathPen::new();
        let mut rev = ReverseContourPen::new(&mut bez);
        draw_closed_triangle(&mut rev);
        rev.flush().unwrap();
        assert_eq!("M100,100 L50,200 L150,200 Z", bez.into_inner().to_svg());
    }

    #[test]
    fn reverse_closed_shape() {
        let mut bez = BezPathPen::new();
        draw_closed_test_shape(&mut bez);
        assert_eq!(
            "M125,100 Q200,150 175,300 C150,150 50,150 25,300 Q0,150 75,100 L100,50 Z",
            bez.into_inner().to_svg()
        );

        let mut bez = BezPathPen::new();
        let mut rev = ReverseContourPen::new(&mut bez);
        draw_closed_test_shape(&mut rev);
        rev.flush().unwrap();
        assert_eq!(
            "M125,100 L100,50 L75,100 Q0,150 25,300 C50,150 150,150 175,300 Q200,150 125,100",
            bez.into_inner().to_svg()
        );
    }

    /// https://github.com/fonttools/fonttools/blob/bf265ce49e0cae6f032420a4c80c31d8e16285b8/Tests/pens/reverseContourPen_test.py#L7
    #[test]
    fn test_reverse_lines() {
        let mut rec = RecordingPen::new();
        let mut rev = ReverseContourPen::new(&mut rec);
        rev.move_to(0.0, 0.0);
        rev.line_to(1.0, 1.0);
        rev.line_to(2.0, 2.0);
        rev.line_to(3.0, 3.0);
        rev.close();
        rev.flush().unwrap();

        assert_eq!(
            &vec![
                PenCommand::MoveTo { x: 0.0, y: 0.0 },
                PenCommand::LineTo { x: 3.0, y: 3.0 },
                PenCommand::LineTo { x: 2.0, y: 2.0 },
                PenCommand::LineTo { x: 1.0, y: 1.0 },
                PenCommand::Close,
            ],
            rec.commands()
        );
    }

    #[test]
    fn test_control_bounds() {
        // a sort of map ping looking thing drawn with a single cubic
        // cbox is wildly different than tight box
        let bez = BezPath::from_svg("M200,300 C50,50 350,50 200,300").unwrap();
        let mut pen = ControlBoundsPen::new();
        write_to_pen(&bez, &mut pen);

        assert_eq!(Some(Rect::new(50.0, 50.0, 350.0, 300.0)), pen.bounds());
        assert!(pen.bounds().unwrap().area() > bez.bounding_box().area());
    }
}
