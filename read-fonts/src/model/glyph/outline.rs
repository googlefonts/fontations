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
