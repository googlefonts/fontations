//! Stores a disjoint collection of ranges over numeric types.
//!
//! Overlapping ranges are automatically merged together.

use core::{
    cmp::{max, min},
    ops::RangeInclusive,
};
use std::collections::BTreeMap;

use types::Fixed;

#[derive(Default)]
pub struct RangeSet<T> {
    // an entry in the map ranges[a] = b implies there is an range [a, b] (inclusive) in this set.
    ranges: BTreeMap<T, T>,
}

pub trait Sequence<T> {
    fn next(&self) -> Option<T>;
}

impl<T> RangeSet<T>
where
    T: Ord + Copy + Sequence<T>,
{
    pub fn insert(&mut self, start: T, end: T) {
        if end < start {
            // ignore or malformed ranges.
            return;
        }

        let mut start = start;
        let mut end = end;

        // There may be up to one intersecting range prior to this new range, check for it and merge if needed.
        if let Some((prev_start, prev_end)) = self.prev_range(start) {
            if range_is_subset(start, end, prev_start, prev_end) {
                return;
            }
            if ranges_overlap_or_adjacent(start, end, prev_start, prev_end) {
                start = min(start, prev_start);
                end = max(end, prev_end);
                self.ranges.remove(&prev_start);
            }
        };

        // There may be one or more ranges proceeding this new range that intersect, find and merge them as needed.
        loop {
            let Some((next_start, next_end)) = self.next_range(start) else {
                // No existing ranges which might overlap, can now insert the current range
                self.ranges.insert(start, end);
                return;
            };

            if range_is_subset(start, end, next_start, next_end) {
                return;
            }
            if ranges_overlap_or_adjacent(start, end, next_start, next_end) {
                start = min(start, next_start);
                end = max(end, next_end);
                self.ranges.remove(&next_start);
            } else {
                self.ranges.insert(start, end);
                return;
            }
        }
    }

    pub fn iter(&'_ self) -> impl Iterator<Item = RangeInclusive<T>> + '_ {
        self.ranges.iter().map(|(a, b)| *a..=*b)
    }

    /// Finds a range in this set with a start greater than or equal to the provided start value.
    fn next_range(&self, start: T) -> Option<(T, T)> {
        let (next_start, next_end) = self.ranges.range(start..).next()?;
        Some((*next_start, *next_end))
    }

    /// Finds a range in this set with a start less than the provided start value.
    fn prev_range(&self, start: T) -> Option<(T, T)> {
        let (next_start, next_end) = self.ranges.range(..start).next_back()?;
        Some((*next_start, *next_end))
    }
}

impl Sequence<u32> for u32 {
    fn next(&self) -> Option<Self> {
        self.checked_add(1)
    }
}

impl Sequence<u16> for u16 {
    fn next(&self) -> Option<Self> {
        self.checked_add(1)
    }
}

impl Sequence<Fixed> for Fixed {
    fn next(&self) -> Option<Self> {
        self.checked_add(Fixed::EPSILON)
    }
}

/// Returns true if the ranges [a_start, a_end] and [b_start, b_end] overlap or are adjacent to each other.
///
/// All bounds are inclusive.
fn ranges_overlap_or_adjacent<T>(a_start: T, a_end: T, b_start: T, b_end: T) -> bool
where
    T: Ord + Sequence<T>,
{
    (a_start <= b_end && b_start <= a_end)
        || (a_end.next() == Some(b_start))
        || (b_end.next() == Some(a_start))
}

/// Returns true if the range [a_start, a_end] is a subset of [b_start, b_end].
///
/// All bounds are inclusive.
fn range_is_subset<T>(a_start: T, a_end: T, b_start: T, b_end: T) -> bool
where
    T: Ord,
{
    a_start >= b_start && a_end <= b_end
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn insert_invalid() {
        let mut map: RangeSet<u32> = Default::default();
        map.insert(12, 11);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![],);
    }

    #[test]
    fn insert_non_overlapping() {
        let mut map: RangeSet<u32> = Default::default();

        map.insert(11, 11);
        map.insert(2, 3);
        map.insert(6, 9);

        assert_eq!(map.iter().collect::<Vec<_>>(), vec![2..=3, 6..=9, 11..=11],);
    }

    #[test]
    fn insert_subset_before() {
        let mut map: RangeSet<u32> = Default::default();

        map.insert(2, 8);
        map.insert(3, 7);

        assert_eq!(map.iter().collect::<Vec<_>>(), vec![2..=8],);
    }

    #[test]
    fn insert_subset_after() {
        let mut map: RangeSet<u32> = Default::default();

        map.insert(2, 8);
        map.insert(2, 7);
        map.insert(2, 8);

        assert_eq!(map.iter().collect::<Vec<_>>(), vec![2..=8],);
    }

    #[test]
    fn insert_overlapping_before() {
        let mut map: RangeSet<u32> = Default::default();

        map.insert(2, 8);
        map.insert(7, 11);

        assert_eq!(map.iter().collect::<Vec<_>>(), vec![2..=11],);
    }

    #[test]
    fn insert_overlapping_after() {
        let mut map: RangeSet<u32> = Default::default();
        map.insert(10, 14);
        map.insert(7, 11);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![7..=14],);

        let mut map: RangeSet<u32> = Default::default();
        map.insert(10, 14);
        map.insert(10, 17);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![10..=17],);
    }

    #[test]
    fn insert_overlapping_multiple_after() {
        let mut map: RangeSet<u32> = Default::default();
        map.insert(10, 14);
        map.insert(16, 17);
        map.insert(7, 16);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![7..=17],);

        let mut map: RangeSet<u32> = Default::default();
        map.insert(10, 14);
        map.insert(16, 17);
        map.insert(10, 16);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![10..=17],);

        let mut map: RangeSet<u32> = Default::default();
        map.insert(10, 14);
        map.insert(16, 17);
        map.insert(10, 17);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![10..=17],);
    }

    #[test]
    fn insert_overlapping_before_and_after() {
        let mut map: RangeSet<u32> = Default::default();

        map.insert(6, 8);
        map.insert(10, 14);
        map.insert(16, 20);

        map.insert(7, 19);

        assert_eq!(map.iter().collect::<Vec<_>>(), vec![6..=20],);
    }

    #[test]
    fn insert_joins_adjacent() {
        let mut map: RangeSet<u32> = Default::default();
        map.insert(6, 8);
        map.insert(9, 10);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![6..=10],);

        let mut map: RangeSet<u32> = Default::default();
        map.insert(9, 10);
        map.insert(6, 8);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![6..=10],);

        let mut map: RangeSet<u32> = Default::default();
        map.insert(6, 8);
        map.insert(10, 10);
        map.insert(9, 9);
        assert_eq!(map.iter().collect::<Vec<_>>(), vec![6..=10],);
    }
}
