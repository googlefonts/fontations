//! Higher level interface for accessing font data.

pub mod metrics;
pub mod pen;

#[cfg(feature = "experimental_font_api")]
mod once;

#[cfg(feature = "experimental_font_api")]
mod font;

#[cfg(feature = "experimental_font_api")]
pub use font::{
    interop as _font_interop, Blob, Font, Format, InstanceBuilder, Kind, NormalizedCoord, Source,
    TableFunction, Tables, Variation,
};
