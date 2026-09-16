//! Error types for font sanitization.

use font_types::Tag;
use thiserror::Error;

/// The severity level of a message emitted during sanitization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageLevel {
    /// An informational or warning message about a non-fatal issue or automatic fix.
    Warning = 1,
    /// An error message explaining why sanitization failed.
    Error = 0,
}

/// Errors that can occur while sanitizing an OpenType font file.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SanitizeError {
    #[error("File exceeds maximum allowed size (1GB)")]
    FileTooLarge,

    #[error("File is too short or truncated: {0}")]
    Truncated(&'static str),

    #[error("Invalid sfntVersion: {0:#010x}")]
    InvalidSfntVersion(u32),

    #[error("Invalid TTC header: {0}")]
    InvalidTtc(&'static str),

    #[error("Requested font index {index} is out of bounds (TTC contains {num_fonts} fonts)")]
    TtcIndexOutOfBounds { index: u32, num_fonts: u32 },

    #[error("Excessive or zero number of tables in font: {0}")]
    InvalidTableCount(u16),

    #[error("Misaligned table '{tag}': offset {offset:#x} is not 4-byte aligned")]
    MisalignedTable { tag: Tag, offset: u32 },

    #[error("Invalid table offset for '{tag}': offset {offset} is out of bounds")]
    InvalidTableOffset { tag: Tag, offset: u32 },

    #[error("Zero-length table '{0}'")]
    ZeroLengthTable(Tag),

    #[error("Table '{tag}' length exceeds 1GB: {length}")]
    TableTooLarge { tag: Tag, length: u32 },

    #[error("Table '{tag}' overruns end of file: offset {offset} + length {length} > file size {file_size}")]
    TableOverrunsFile {
        tag: Tag,
        offset: u32,
        length: u32,
        file_size: usize,
    },

    #[error("Overlapping tables detected in font file")]
    OverlappingTables,

    #[error("Missing required table '{0}'")]
    MissingRequiredTable(Tag),

    #[error("Failed to parse or validate table '{tag}': {message}")]
    TableParseError { tag: Tag, message: String },

    #[error("Failed to serialize table '{tag}': {message}")]
    TableSerializeError { tag: Tag, message: String },

    #[error("No supported glyph data tables present (requires glyf/loca or CFF/CFF2)")]
    NoGlyphData,

    #[error("Font sanitization failed: {0}")]
    Other(String),
}
