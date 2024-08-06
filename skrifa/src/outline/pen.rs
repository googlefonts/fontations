//! Types for collecting the output when drawing a glyph outline.

use alloc::{string::String, vec::Vec};
use core::fmt::{self, Write};

/// Interface for accepting a sequence of path commands.
pub trait OutlinePen {
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

/// Single element of a path.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub enum PathElement {
    /// Begin a new subpath at (x, y).
    MoveTo { x: f32, y: f32 },
    /// Draw a line from the current point to (x, y).
    LineTo { x: f32, y: f32 },
    /// Draw a quadratic bezier from the current point with a control point at
    /// (cx0, cy0) and ending at (x, y).
    QuadTo { cx0: f32, cy0: f32, x: f32, y: f32 },
    /// Draw a cubic bezier from the current point with control points at
    /// (cx0, cy0) and (cx1, cy1) and ending at (x, y).
    CurveTo {
        cx0: f32,
        cy0: f32,
        cx1: f32,
        cy1: f32,
        x: f32,
        y: f32,
    },
    /// Close the current subpath.
    Close,
}

/// Style for path conversion.
///
/// The order to process points in a glyf point stream is ambiguous when the
/// first point is off-curve. Major implementations differ. Which one would
/// you like to match?
///
/// **If you add a new one make sure to update the fuzzer.**
#[derive(Debug, Default, Copy, Clone)]
pub enum PathStyle {
    /// If the first point is off-curve, check if the last is on-curve
    /// If it is, start there. If it isn't, start at the implied midpoint
    /// between first and last.
    #[default]
    FreeType,
    /// If the first point is off-curve, check if the second is on-curve.
    /// If it is, start there. If it isn't, start at the implied midpoint
    /// between first and second.
    ///
    /// Matches hb-draw's interpretation of a point stream.
    HarfBuzz,
}

impl OutlinePen for Vec<PathElement> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.push(PathElement::MoveTo { x, y })
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(PathElement::LineTo { x, y })
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.push(PathElement::QuadTo { cx0, cy0, x, y })
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.push(PathElement::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        })
    }

    fn close(&mut self) {
        self.push(PathElement::Close)
    }
}

/// Pen that drops all drawing output into the ether.
pub struct NullPen;

impl OutlinePen for NullPen {
    fn move_to(&mut self, _x: f32, _y: f32) {}
    fn line_to(&mut self, _x: f32, _y: f32) {}
    fn quad_to(&mut self, _cx0: f32, _cy0: f32, _x: f32, _y: f32) {}
    fn curve_to(&mut self, _cx0: f32, _cy0: f32, _cx1: f32, _cy1: f32, _x: f32, _y: f32) {}
    fn close(&mut self) {}
}

/// Pen that generates SVG style path data.
#[derive(Clone, Default, Debug)]
pub struct SvgPen(String);

impl SvgPen {
    /// Clears the content of the internal string.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    fn maybe_push_space(&mut self) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
    }
}

impl core::ops::Deref for SvgPen {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl OutlinePen for SvgPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.maybe_push_space();
        let _ = write!(self.0, "M{x:.1},{y:.1}");
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.maybe_push_space();
        let _ = write!(self.0, "L{x:.1},{y:.1}");
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.maybe_push_space();
        let _ = write!(self.0, "Q{cx0:.1},{cy0:.1} {x:.1},{y:.1}");
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.maybe_push_space();
        let _ = write!(self.0, "C{cx0:.1},{cy0:.1} {cx1:.1},{cy1:.1} {x:.1},{y:.1}");
    }

    fn close(&mut self) {
        self.maybe_push_space();
        self.0.push('Z');
    }
}

impl AsRef<str> for SvgPen {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<String> for SvgPen {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<SvgPen> for String {
    fn from(value: SvgPen) -> Self {
        value.0
    }
}

impl fmt::Display for SvgPen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
