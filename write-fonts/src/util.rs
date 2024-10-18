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

/// Compare two floats for equality using relative and absolute tolerances.
///
/// This is useful when porting from Python where `math.isclose` is common.
///
/// References:
/// - [PEP425](https://peps.python.org/pep-0485/)
/// - [math.isclose](https://docs.python.org/3/library/math.html#math.isclose)
#[derive(Clone, Copy, Debug)]
pub struct FloatComparator {
    // TODO: Make it generic over T: Float? Need to add num-traits as a dependency
    rel_tol: f64,
    abs_tol: f64,
}

impl FloatComparator {
    /// Create a new FloatComparator with the specified relative and absolute tolerances.
    pub fn new(rel_tol: f64, abs_tol: f64) -> Self {
        Self { rel_tol, abs_tol }
    }

    #[inline]
    /// Return true if a and b are close according to the specified tolerances.
    pub fn isclose(self, a: f64, b: f64) -> bool {
        if a == b {
            return true;
        }
        if !a.is_finite() || !b.is_finite() {
            return false;
        }
        // The https://peps.python.org/pep-0485/ describes the algorithm used as:
        //   abs(a-b) <= max( rel_tol * max(abs(a), abs(b)), abs_tol )
        // In Rust, that would literally be:
        //   diff <= f64::max(self.rel_tol * f64::max(f64::abs(a), f64::abs(b)), self.abs_tol)
        // However below I use || instead of max(), since the logic works out the same and
        // should be a bit faster. In the PEP it's referred to as Boost's 'weak' formulation.
        let diff = (a - b).abs();
        diff <= (self.rel_tol * b).abs() || diff <= (self.rel_tol * a).abs() || diff <= self.abs_tol
    }
}

impl Default for FloatComparator {
    /// Create a new FloatComparator with `rel_to=1e-9` and `abs_tol=0.0`.
    fn default() -> Self {
        // same defaults as Python's math.isclose so we can match fontTools
        Self::new(1e-9, 0.0)
    }
}

/// Compare two floats for equality using the default FloatComparator.
///
/// To use different relative or absolute tolerances, create a FloatComparator
/// and use its `isclose` method.
pub fn isclose(a: f64, b: f64) -> bool {
    FloatComparator::default().isclose(a, b)
}

/// Search range values used in various tables
#[derive(Clone, Copy, Debug)]
pub struct SearchRange {
    pub search_range: u16,
    pub entry_selector: u16,
    pub range_shift: u16,
}

impl SearchRange {
    //https://github.com/fonttools/fonttools/blob/729b3d2960ef/Lib/fontTools/ttLib/ttFont.py#L1147
    /// calculate searchRange, entrySelector, and rangeShift
    ///
    /// these values are used in various places, such as [the base table directory]
    /// and [cmap format 4].
    ///
    /// [the base table directory]: https://learn.microsoft.com/en-us/typography/opentype/spec/otff#table-directory
    /// [cmap format 4]: https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values
    pub fn compute(n_items: usize, item_size: usize) -> Self {
        let entry_selector = (n_items as f64).log2().floor() as usize;
        let search_range = (2.0_f64.powi(entry_selector as i32) * item_size as f64) as usize;
        // The result doesn't really make sense with 0 tables but ... let's at least not fail
        let range_shift = (n_items * item_size).saturating_sub(search_range);
        SearchRange {
            search_range: search_range.try_into().unwrap(),
            entry_selector: entry_selector.try_into().unwrap(),
            range_shift: range_shift.try_into().unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// based on example at
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values>
    #[test]
    fn simple_search_range() {
        let SearchRange {
            search_range,
            entry_selector,
            range_shift,
        } = SearchRange::compute(39, 2);
        assert_eq!((search_range, entry_selector, range_shift), (64, 5, 14));
    }

    #[test]
    fn search_range_no_crashy() {
        let foo = SearchRange::compute(0, 0);
        assert_eq!(
            (foo.search_range, foo.entry_selector, foo.range_shift),
            (0, 0, 0)
        )
    }
}
