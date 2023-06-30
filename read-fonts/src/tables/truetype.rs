//! Support for processing TrueType outlines.

use std::fmt;

use crate::{types::GlyphId, ReadError};

mod deltas;
mod mem;
mod outline;
mod scale;

pub use mem::ScalerMemory;
pub use outline::{HintOutline, ScalerGlyph, ScalerOutline};
pub use scale::Scaler;

/// Recursion limit for processing composite outlines.
///
/// In reality, most fonts contain shallow composite graphs with a nesting
/// depth of 1 or 2. This is set as a hard limit to avoid stack overflow
/// and infinite recursion.
pub const COMPOSITE_RECURSION_LIMIT: usize = 32;

/// Number of phantom points generated at the end of an outline.
pub const PHANTOM_POINT_COUNT: usize = 4;

/// Errors that may occur when scaling glyphs.
#[derive(Clone, Debug)]
pub enum Error {
    /// The requested glyph was not present in the font.
    GlyphNotFound(GlyphId),
    /// Exceeded a recursion limit when loading a glyph.
    RecursionLimitExceeded(GlyphId),
    /// Exceeded memory limits when loading a glyph.
    InsufficientMemory,
    /// An anchor point had invalid indices.
    InvalidAnchorPoint(GlyphId, u16),
    /// Error occured during hinting.
    // TODO: add real hinting error type
    HintingFailed(GlyphId),
    /// Error occured when reading font data.
    Read(ReadError),
}

impl From<ReadError> for Error {
    fn from(e: ReadError) -> Self {
        Self::Read(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::GlyphNotFound(gid) => write!(f, "glyph {gid} was not found in the given font"),
            Self::RecursionLimitExceeded(gid) => write!(
                f,
                "recursion limit ({}) exceeded when loading composite component {gid}",
                COMPOSITE_RECURSION_LIMIT,
            ),
            Self::InsufficientMemory => write!(f, "exceeded memory limits"),
            Self::InvalidAnchorPoint(gid, index) => write!(
                f,
                "invalid anchor point index ({index}) for composite glyph {gid}",
            ),
            Self::HintingFailed(gid) => write!(f, "error when hinting glyph {gid}"),
            Self::Read(e) => write!(f, "{e}"),
        }
    }
}
