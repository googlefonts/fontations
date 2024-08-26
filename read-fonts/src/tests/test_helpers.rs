//! small utilities used in tests

use crate::{FontData, Scalar};

/// A convenience type for generating a buffer of big-endian bytes.
#[derive(Debug, Clone, Default)]
pub struct BeBuffer(Vec<u8>);

impl BeBuffer {
    pub fn new() -> Self {
        Default::default()
    }

    /// The current length of the buffer in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the buffer contains zero bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return a reference to the contents of the buffer
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Write any scalar to this buffer.
    pub fn push(mut self, item: impl Scalar) -> Self {
        self.0.extend(item.to_raw().as_ref());
        self
    }

    /// Write multiple scalars into the buffer
    pub fn extend<T: Scalar>(mut self, iter: impl IntoIterator<Item = T>) -> Self {
        for item in iter {
            self.0.extend(item.to_raw().as_ref());
        }
        self
    }

    pub fn font_data(&self) -> FontData {
        FontData::new(&self.0)
    }
}

impl std::ops::Deref for BeBuffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
