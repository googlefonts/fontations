//! Higher level interface for accessing font data.

pub mod charmap;
pub mod glyph;
pub mod metrics;
pub mod name;

mod error;
mod font;
mod once;

// Do our best to not expose this to users through docs or rust-analyzer.
#[doc(inline)]
pub use error::Error;
#[doc(hidden)]
pub use font::interop as _font_interop;
#[doc(inline)]
pub use font::{
    Blob, Font, Format, InstanceBuilder, Kind, NormalizedCoord, Source, TableFunction, Tables,
    Variation,
};
