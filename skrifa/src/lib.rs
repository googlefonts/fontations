//! A robust, ergonomic, high performance crate for OpenType fonts.
//!  
//! Skrifa is a mid level library that provides access to various types
//! of [`metadata`](meta) contained in a font as well as support for
//! [`scaling`](scale) of glyph outlines.
//!
//! It is described as "mid level" because the library is designed to sit
//! above low level font parsing (provided by [`read-fonts`](https://crates.io/crates/read-fonts))
//! and below a higher level text layout engine.
//!
//! See the [readme](https://github.com/dfrg/fontations/blob/main/skrifa/README.md) for additional
//! details.

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
