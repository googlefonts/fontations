use read_fonts::{
    tables::glyf::ToPathError, tables::postscript::Error as PostScriptError, types::GlyphId,
    ReadError,
};

use std::fmt;

/// Errors that may occur when scaling glyphs.
#[derive(Clone, Debug)]
pub enum ScalerError {
    /// No viable sources were available.
    NoSources,
    /// The requested glyph was not present in the font.
    GlyphNotFound(GlyphId),
    /// Exceeded memory limits when loading a glyph.
    InsufficientMemory,
    /// Exceeded a recursion limit when loading a glyph.
    RecursionLimitExceeded(GlyphId),
    /// Error occured during hinting.
    HintingFailed(GlyphId),
    /// An anchor point had invalid indices.
    InvalidAnchorPoint(GlyphId, u16),
    /// Error occurred while loading a PostScript (CFF/CFF2) glyph.
    PostScript(PostScriptError),
    /// Conversion from outline to path failed.
    ToPath(ToPathError),
    /// Error occured when reading font data.
    Read(ReadError),
}

impl From<ToPathError> for ScalerError {
    fn from(e: ToPathError) -> Self {
        Self::ToPath(e)
    }
}

impl From<ReadError> for ScalerError {
    fn from(e: ReadError) -> Self {
        Self::Read(e)
    }
}

impl From<PostScriptError> for ScalerError {
    fn from(value: PostScriptError) -> Self {
        Self::PostScript(value)
    }
}

impl fmt::Display for ScalerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NoSources => write!(f, "No glyph sources are available for the given font"),
            Self::GlyphNotFound(gid) => write!(f, "Glyph {gid} was not found in the given font"),
            Self::InsufficientMemory => write!(f, "exceeded memory limits"),
            Self::RecursionLimitExceeded(gid) => write!(
                f,
                "Recursion limit ({}) exceeded when loading composite component {gid}",
                super::GLYF_COMPOSITE_RECURSION_LIMIT,
            ),
            Self::HintingFailed(gid) => write!(f, "Bad hinting bytecode for glyph {gid}"),
            Self::InvalidAnchorPoint(gid, index) => write!(
                f,
                "Invalid anchor point index ({index}) for composite glyph {gid}",
            ),
            Self::PostScript(e) => write!(f, "{e}"),
            Self::ToPath(e) => write!(f, "{e}"),
            Self::Read(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ScalerError {}

/// Result type for errors that may occur when loading glyphs.
pub type Result<T> = core::result::Result<T, ScalerError>;
