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
