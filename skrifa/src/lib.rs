//! A robust, ergonomic, high performance crate for OpenType fonts.
//!  
//! Skrifa is a mid level library that provides access to various types
//! of [`metadata`](meta) contained in a font as well as support for
//! [`scaling`](scale) (extraction) of glyph outlines.
//!
//! It is described as "mid level" because the library is designed to sit
//! above low level font parsing (provided by [`read-fonts`](https://crates.io/crates/read-fonts))
//! and below a higher level text layout engine.
//!
//! A simple end to end example of usage follows:
//!
//! ```no_run
//! use std::path::Path;
//! use skrifa::Size;
//! use skrifa::meta::charmap::Charmap;
//! use skrifa::scale::{Context, Pen};
//! use read_fonts::FontRef;
//!
//! // We want to use Recursive (https://fonts.google.com/specimen/Recursive/tester)
//! // Assumes https://github.com/google/fonts at ../fonts
//! let font_file = Path::new("../fonts/ofl/recursive/Recursive[CASL,CRSV,MONO,slnt,wght].ttf");
//!
//! // If you were confident it wouldn't change you could memory map the file
//! // For example, for an OS font on an immutable system image
//! let buf = std::fs::read(font_file).unwrap();
//! let font = FontRef::new(&buf).unwrap();
//!
//! // Create some sort of pen and load outlines into it!
//!
//! fn load_outline(font: &read_fonts::FontRef, ch: char, pen: &mut impl Pen) {
//!     // Lookup the glyph id from the character map
//!     // Typically this would be done for you by a shaper, such as https://github.com/harfbuzz/harfbuzz
//!     // What's glyph id or a character map? - see https://rsheeter.github.io/font101/#glyph-ids-and-the-cmap-table
//!     let glyph_id = Charmap::new(font).map(ch as u32).unwrap();
//!
//!     // Now let's get the outline of the shape so we can draw it, write it out as an svg, etc
//!     // The outline gets pushed into a Pen, which converts it to our desired output format
//!     let mut cx = Context::new();
//!
//!     // Create a scaler with our desired settings
//!     let mut scaler = cx
//!        .new_scaler( )
//!        .size(Size::new(16.0))  // 16px
//!        .variation_settings(&[("MONO", 1.0)])  // kindly be monospace
//!        .build(font);
//!
//!     // Extract the outline into the pen
//!     scaler.outline(glyph_id, pen).unwrap();
//! }
//! ```
//!
//! See the [`scaling`](scale) module documentation for a more in depth explanation
//! of the types and steps involved in extracting outlines.
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
