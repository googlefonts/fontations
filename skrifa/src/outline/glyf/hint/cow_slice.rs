//! Provides copy-on-write containers for CVT and storage area.
//!
//! See <https://gitlab.freedesktop.org/freetype/freetype/-/merge_requests/23>

use super::HintErrorKind;

pub struct CowSlice<'a> {
    /// True if we've initialized the mutable slice
    has_mut: bool,
    data: &'a [i32],
    data_mut: &'a mut [i32],
}

impl<'a> CowSlice<'a> {
    pub fn new(data: &'a [i32], data_mut: &'a mut [i32]) -> Self {
        assert_eq!(data.len(), data_mut.len());
        Self {
            has_mut: false,
            data,
            data_mut,
        }
    }

    pub fn new_mut(data_mut: &'a mut [i32]) -> Self {
        Self {
            has_mut: true,
            data: &[],
            data_mut,
        }
    }

    pub fn get(&self, index: usize) -> Option<i32> {
        if self.has_mut {
            self.data_mut.get(index).copied()
        } else {
            self.data.get(index).copied()
        }
    }

    pub fn set(&mut self, index: usize, value: i32) -> Option<()> {
        // Copy from immutable to mutable buffer if we haven't already
        if !self.has_mut {
            self.data_mut.copy_from_slice(self.data);
            self.has_mut = true;
        }
        *self.data_mut.get_mut(index)? = value;
        Some(())
    }
}

pub struct Cvt<'a>(CowSlice<'a>);

impl<'a> From<CowSlice<'a>> for Cvt<'a> {
    fn from(value: CowSlice<'a>) -> Self {
        Self(value)
    }
}

impl<'a> Cvt<'a> {
    pub fn get(&self, index: usize) -> Result<i32, HintErrorKind> {
        self.0
            .get(index)
            .ok_or(HintErrorKind::InvalidCvtIndex(index))
    }

    pub fn set(&mut self, index: usize, value: i32) -> Result<(), HintErrorKind> {
        self.0
            .set(index, value)
            .ok_or(HintErrorKind::InvalidCvtIndex(index))
    }
}

pub struct Storage<'a>(CowSlice<'a>);

impl<'a> Storage<'a> {
    pub fn get(&self, index: usize) -> Result<i32, HintErrorKind> {
        self.0
            .get(index)
            .ok_or(HintErrorKind::InvalidStorageIndex(index))
    }

    pub fn set(&mut self, index: usize, value: i32) -> Result<(), HintErrorKind> {
        self.0
            .set(index, value)
            .ok_or(HintErrorKind::InvalidStorageIndex(index))
    }
}

impl<'a> From<CowSlice<'a>> for Storage<'a> {
    fn from(value: CowSlice<'a>) -> Self {
        Self(value)
    }
}
