use std::ops::Range;

use crate::{ExactSized, FromBeBytes};

/// Some bytes.
pub struct Blob<'a>(&'a [u8]);

impl<'a> Blob<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, range: Range<usize>) -> Option<Self> {
        self.0.get(range).map(Self)
    }

    pub fn read<T: ExactSized + FromBeBytes>(&self, offset: usize) -> Option<T> {
        self.0.get(offset..offset + T::SIZE).and_then(T::from_bytes)
    }

    pub unsafe fn read_unchecked<T: ExactSized + FromBeBytes>(&self, offset: usize) -> Option<T> {
        T::from_bytes(self.0.get_unchecked(offset..offset + T::SIZE))
    }
}
