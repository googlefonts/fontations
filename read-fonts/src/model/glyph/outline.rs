//! Scalable glyph outlines.
//!
//! Outlines are drawn as paths into a [`PathSink`]. A `Vec` of
//! [`PathElement`] collects a path, [`ControlBoundsSink`] measures it,
//! [`TransformSink`] transforms it, [`SvgSink`] writes it as SVG path data,
//! and [`NullSink`] discards it. An [`UnscaledOutlineSink`] receives an
//! outline's points in font units instead.

mod path;
mod unscaled;

#[doc(inline)]
pub use path::{ControlBoundsSink, NullSink, PathElement, PathSink, SvgSink, TransformSink};
#[doc(inline)]
pub use unscaled::UnscaledOutlineSink;

/// A table that provides outlines.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum OutlineSource {
    /// TrueType outlines in `glyf`.
    Glyf,
    /// Variable PostScript outlines in `CFF2`.
    Cff2,
    /// PostScript outlines in a `CFF` table or a standalone CFF font.
    Cff,
    /// Charstrings of a Type 1 font.
    Type1,
}
