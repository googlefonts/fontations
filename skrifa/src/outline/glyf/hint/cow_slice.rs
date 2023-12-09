use super::HintErrorKind;

/// Copy-on-write buffers for CVT and storage.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/merge_requests/23>
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

    pub fn get(&self, index: usize) -> Result<i32, HintErrorKind> {
        if self.has_mut {
            self.data_mut.get(index).copied()
        } else {
            self.data.get(index).copied()
        }
        .ok_or(HintErrorKind::InvalidCvtIndex(index))
    }

    pub fn set(&mut self, index: usize, value: i32) -> Result<(), HintErrorKind> {
        // Copy from immutable to mutable buffer if we haven't already
        if !self.has_mut {
            self.data_mut.copy_from_slice(self.data);
            self.has_mut = true;
        }
        *self
            .data_mut
            .get_mut(index)
            .ok_or(HintErrorKind::InvalidCvtIndex(index))? = value;
        Ok(())
    }
}
