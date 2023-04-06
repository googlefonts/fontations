#![forbid(unsafe_code)]
// TODO: this is temporary-- remove when hinting is added.
#![allow(dead_code, unused_imports, unused_variables)]

/// Expose our "raw" underlying parser crate.
pub extern crate read_fonts as raw;

pub mod meta;
#[cfg(feature = "scale")]
pub mod scale;

mod coords;
mod setting;
mod size;
mod unique_id;

pub use coords::NormalizedCoords;
pub use setting::{Setting, VariationSetting};
pub use size::Size;
pub use unique_id::UniqueId;

/// Type for a glyph identifier.
pub type GlyphId = read_fonts::types::GlyphId;

/// Type for a 4-byte tag used to identify font tables and other resources.
pub type Tag = read_fonts::types::Tag;

/// Type for a normalized variation coordinate.
pub type NormalizedCoord = read_fonts::types::F2Dot14;

#[doc(inline)]
pub use meta::MetadataProvider;
