//! Misc utility functions

/// Iterator that iterates over a vector of iterators simultaneously.
///
/// Adapted from <https://stackoverflow.com/a/55292215>
pub struct MultiZip<I: Iterator>(Vec<I>);

impl<I: Iterator> MultiZip<I> {
    /// Create a new MultiZip from a vector of iterators
    pub fn new(vec_of_iters: Vec<I>) -> Self {
        Self(vec_of_iters)
    }
}

impl<I: Iterator> Iterator for MultiZip<I> {
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.iter_mut().map(Iterator::next).collect()
    }
}

/// Read next or previous, wrapping if we go out of bounds.
///
/// This is particularly useful when porting from Python where reading
/// idx - 1, with -1 meaning last, is common.
pub trait WrappingGet<T> {
    fn wrapping_next(&self, idx: usize) -> &T;
    fn wrapping_prev(&self, idx: usize) -> &T;
}

impl<T> WrappingGet<T> for &[T] {
    fn wrapping_next(&self, idx: usize) -> &T {
        &self[match idx {
            _ if idx == self.len() - 1 => 0,
            _ => idx + 1,
        }]
    }

    fn wrapping_prev(&self, idx: usize) -> &T {
        &self[match idx {
            _ if idx == 0 => self.len() - 1,
            _ => idx - 1,
        }]
    }
}
