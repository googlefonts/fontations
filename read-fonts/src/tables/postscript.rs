//! PostScript (CFF and CFF2) common tables.

use std::fmt;

mod blend;
mod index;
mod stack;

include!("../../generated/generated_postscript.rs");

pub use blend::BlendState;
pub use index::Index;
pub use stack::Stack;

/// Errors that are specific to PostScript processing.
#[derive(Clone, Debug)]
pub enum Error {
    InvalidIndexOffsetSize(u8),
    ZeroOffsetInIndex,
    InvalidVariationStoreIndex(u16),
    StackOverflow,
    StackUnderflow,
    InvalidStackAccess(usize),
    ExpectedI32StackEntry(usize),
    Read(ReadError),
}

impl From<ReadError> for Error {
    fn from(value: ReadError) -> Self {
        Self::Read(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIndexOffsetSize(size) => {
                write!(f, "invalid offset size of {size} for INDEX (expected 1-4)")
            }
            Self::ZeroOffsetInIndex => {
                write!(f, "invalid offset of 0 in INDEX (must be >= 1)")
            }
            Self::InvalidVariationStoreIndex(index) => {
                write!(
                    f,
                    "variation store index {index} referenced an invalid variation region"
                )
            }
            Self::StackOverflow => {
                write!(f, "attempted to push a value to a full stack")
            }
            Self::StackUnderflow => {
                write!(f, "attempted to pop a value from an empty stack")
            }
            Self::InvalidStackAccess(index) => {
                write!(f, "invalid stack access for index {index}")
            }
            Self::ExpectedI32StackEntry(index) => {
                write!(f, "attempted to read an integer at stack index {index}, but found a fixed point value")
            }
            Self::Read(err) => write!(f, "{err}"),
        }
    }
}