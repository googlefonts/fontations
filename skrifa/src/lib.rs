#![forbid(unsafe_code)]
// TODO: this is temporary-- remove when hinting is added.
#![allow(dead_code, unused_imports, unused_variables)]

/// Expose our "raw" underlying parser crate.
pub extern crate read_fonts as raw;

pub mod meta;

#[cfg(feature = "scale")]
pub mod scale;

/// Type for a normalized variation coordinate.
pub type NormalizedCoord = read_fonts::types::F2Dot14;

/// Type for a glyph identifier.
pub type GlyphId = read_fonts::types::GlyphId;

#[doc(inline)]
pub use meta::MetadataProvider;
