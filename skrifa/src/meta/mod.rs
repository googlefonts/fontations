//! Accessors for various types of font metadata.

pub mod charmap;
pub mod metrics;
pub mod strings;

mod provider;

pub use provider::MetadataProvider;
