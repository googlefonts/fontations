use std::ops::Range;

use crate::{ExactSized, FromBeBytes};

/// Some bytes.
#[derive(Clone, Debug)]
pub struct Blob<'a>(&'a [u8]);

impl<'a> Blob<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &'a [u8] {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, range: Range<usize>) -> Option<Self> {
        self.0.get(range).map(Self)
    }

    pub unsafe fn get_unchecked(&self, range: Range<usize>) -> Self {
        Self(self.0.get_unchecked(range))
    }

    pub fn read<T: ExactSized + FromBeBytes>(&self, offset: usize) -> Option<T> {
        self.0.get(offset..offset + T::SIZE).and_then(T::from_bytes)
    }

    /// attempt to read type `T` at offset, without bounds checking.
    ///
    /// # Safety
    ///
    /// This should only be used if you can guarantee that `offset+T::SIZE` is in
    /// bounds for this type. In general this only makes sense if you are calling
    /// this method from inside the impl of a type that contains a private
    /// `Blob`, and which has checked the length of the data at construction time.
    pub unsafe fn read_unchecked<T: ExactSized + FromBeBytes>(&self, offset: usize) -> Option<T> {
        T::from_bytes(self.0.get_unchecked(offset..offset + T::SIZE))
    }
}
