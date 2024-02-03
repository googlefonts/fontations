//! State for managing active programs and decoding instructions.

mod args;

pub use args::Args;

#[cfg(test)]
pub(crate) use args::MockArgs;
