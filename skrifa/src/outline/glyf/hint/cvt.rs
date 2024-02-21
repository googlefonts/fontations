//! Control value table.

use super::{cow_slice::CowSlice, error::HintErrorKind};

/// Backing store for the control value table.
///
/// This is just a wrapper for [`CowSlice`] that converts out of bounds
/// accesses to appropriate errors.
pub struct Cvt<'a>(CowSlice<'a>);

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

impl<'a> From<CowSlice<'a>> for Cvt<'a> {
    fn from(value: CowSlice<'a>) -> Self {
        Self(value)
    }
}
