//! Runtime autohinting support.

// Remove when the work is complete.
#![allow(dead_code, unused)]

mod axis;
mod cycling;
mod hint;
mod instance;
mod latin;
mod metrics;
mod outline;
mod style;

pub use instance::GlyphStyles;
pub(crate) use instance::Instance;
