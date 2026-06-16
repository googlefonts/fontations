//! Higher level interface for accessing font data.

pub mod pen;

mod font;
mod once;

#[cfg(feature = "experimental_font_api")]
pub use font::{
    interop as _font_interop, Font, FontBlob, FontFeatureVariations, FontFormat, FontInstance,
    FontInstanceBuilder, FontKind, FontSource, FontTables, FontVariation, NormalizedCoord,
};
