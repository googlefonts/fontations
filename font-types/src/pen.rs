//! Types for working with sequences of path commands.

use core::{
    fmt,
    iter::{self, Extend},
};

/// Interface for accepting a sequence of path commands.
///
/// This is a general abstraction to unify ouput for processes that decode and/or
/// transform outlines.
///
/// Roughly equivalent to [AbstractPen](https://github.com/fonttools/fonttools/blob/78e10d8b42095b709cd4125e592d914d3ed1558e/Lib/fontTools/pens/basePen.py#L54)
/// in FontTools.
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

/// Captures commands to [Pen] to facilitate implementations that buffer commands.
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PenCommand {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    QuadTo {
        cx0: f32,
        cy0: f32,
        x: f32,
        y: f32,
    },
    CurveTo {
        cx0: f32,
        cy0: f32,
        cx1: f32,
        cy1: f32,
        x: f32,
        y: f32,
    },
    Close,
}

impl PenCommand {
    pub fn apply_to<T: Pen>(&self, pen: &mut T) {
        match *self {
            PenCommand::MoveTo { x, y } => pen.move_to(x, y),
            PenCommand::LineTo { x, y } => pen.line_to(x, y),
            PenCommand::QuadTo { cx0, cy0, x, y } => pen.quad_to(cx0, cy0, x, y),
            PenCommand::CurveTo {
                cx0,
                cy0,
                cx1,
                cy1,
                x,
                y,
            } => pen.curve_to(cx0, cy0, cx1, cy1, x, y),
            PenCommand::Close => pen.close(),
        }
    }

    /// The directly stated - not implied - end point of the command.
    ///
    /// Notably, Close does have an end point but it is not directly stated so it returns None.
    pub fn end_point(&self) -> Option<(f32, f32)> {
        match *self {
            PenCommand::MoveTo { x, y }
            | PenCommand::LineTo { x, y }
            | PenCommand::QuadTo { x, y, .. }
            | PenCommand::CurveTo { x, y, .. } => Some((x, y)),
            PenCommand::Close => None,
        }
    }
}

/// Pen adapter that outputs commands to SVG path data.
///
/// The target may be any type that implements `fmt::Write` such as
/// `String`.
pub struct SvgPen<T> {
    target: T,
    space: &'static str,
}

impl<T> SvgPen<T> {
    pub fn new(target: T) -> Self {
        Self { target, space: "" }
    }

    pub fn into_inner(self) -> T {
        self.target
    }
}

impl<T: fmt::Write> Pen for SvgPen<T> {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.target, "{}M{},{}", self.space, x, y);
        self.space = " ";
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.target, "{}L{},{}", self.space, x, y);
        self.space = " ";
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let _ = write!(self.target, "{}Q{},{} {},{}", self.space, cx0, cy0, x, y);
        self.space = " ";
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let _ = write!(
            self.target,
            "{}C{},{} {},{} {},{}",
            self.space, cx0, cy0, cx1, cy1, x, y
        );
        self.space = " ";
    }

    fn close(&mut self) {
        let _ = write!(self.target, "{}Z", self.space);
        self.space = " ";
    }
}

/// Pen adapter that collects commands into a target buffer.
///
/// The target may be any type that implements `Extend<PenCommand>` such as
/// `Vec<PenCommand>`.
pub struct BufferPen<T> {
    target: T,
}

impl<T> BufferPen<T> {
    pub fn new(target: T) -> Self {
        Self { target }
    }

    pub fn into_inner(self) -> T {
        self.target
    }
}

impl<T: Extend<PenCommand>> Pen for BufferPen<T> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.target.extend(iter::once(PenCommand::MoveTo { x, y }));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.target.extend(iter::once(PenCommand::LineTo { x, y }));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.target
            .extend(iter::once(PenCommand::QuadTo { cx0, cy0, x, y }));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.target.extend(iter::once(PenCommand::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        }));
    }

    fn close(&mut self) {
        self.target.extend(iter::once(PenCommand::Close));
    }
}

#[cfg(test)]
mod tests {
    use super::{BufferPen, PenCommand, SvgPen};

    const TEST_COMMANDS: &[PenCommand] = &[
        PenCommand::MoveTo { x: 1.0, y: 2.5 },
        PenCommand::LineTo { x: 42.0, y: 20.0 },
        PenCommand::QuadTo {
            cx0: 0.5,
            cy0: 0.5,
            x: 1.0,
            y: 2.0,
        },
        PenCommand::CurveTo {
            cx0: 1.2,
            cy0: 2.3,
            cx1: 3.4,
            cy1: 4.5,
            x: 5.6,
            y: 6.7,
        },
        PenCommand::Close,
    ];

    #[test]
    fn pen_to_svg() {
        let mut pen = SvgPen::new(String::default());
        for command in TEST_COMMANDS {
            command.apply_to(&mut pen);
        }
        let buf = pen.into_inner();
        assert_eq!(buf, "M1,2.5 L42,20 Q0.5,0.5 1,2 C1.2,2.3 3.4,4.5 5.6,6.7 Z");
    }

    #[test]
    fn pen_to_buffer() {
        let mut pen = BufferPen::new(vec![]);
        for command in TEST_COMMANDS {
            command.apply_to(&mut pen);
        }
        let commands = pen.into_inner();
        assert_eq!(&commands, TEST_COMMANDS);
    }
}
