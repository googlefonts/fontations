//! Autohinting specific metrics.

// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afblue.h#L328>
pub(super) const MAX_BLUES: usize = 8;

// FreeType keeps a single array of blue values per metrics set
// and mutates when the scale factor changes. We'll separate them so
// that we can reuse unscaled metrics as immutable state without
// recomputing them (which is the expensive part).
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L77>
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub(super) struct UnscaledBlue {
    pub position: i32,
    pub overshoot: i32,
    pub ascender: i32,
    pub descender: i32,
    pub flags: u32,
}

pub(super) type UnscaledBlues = ArrayBuf<UnscaledBlue, MAX_BLUES>;

// There are a few uses of this sort of thing where we have
// a predefined max length but a variable size at runtime.
#[derive(Clone)]
pub(super) struct ArrayBuf<T, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T, const N: usize> ArrayBuf<T, N> {
    pub fn push(&mut self, value: T) {
        if self.len < N {
            self.items[self.len] = value;
            self.len += 1;
        }
    }

    pub fn values(&self) -> &[T] {
        &self.items[..self.len]
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        &mut self.items[..self.len]
    }
}

impl<T: Copy + Default, const N: usize> Default for ArrayBuf<T, N> {
    fn default() -> Self {
        Self {
            items: [T::default(); N],
            len: 0,
        }
    }
}

impl<T: core::fmt::Debug, const N: usize> core::fmt::Debug for ArrayBuf<T, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.values()).finish()
    }
}
