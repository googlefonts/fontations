//! A fast & efficient ordered set for unsigned integers.

use super::bitpage::BitPage;
use super::bitpage::PAGE_BITS;
use std::cell::Cell;
use std::hash::Hash;
use std::ops::RangeInclusive;

// log_2(PAGE_BITS)
const PAGE_BITS_LOG_2: u32 = PAGE_BITS.ilog2();

/// An ordered integer (u32) set.
#[derive(Clone, Debug)]
pub(crate) struct BitSet {
    // TODO(garretrieger): consider a "small array" type instead of Vec.
    pages: Vec<BitPage>,
    page_map: Vec<PageInfo>,
    len: Cell<usize>, // TODO(garretrieger): use an option instead of a sentinel.
}

impl BitSet {
    /// Add val as a member of this set.
    pub(crate) fn insert(&mut self, val: u32) -> bool {
        let page = self.ensure_page_for_mut(val);
        let ret = page.insert(val);
        self.mark_dirty();
        ret
    }

    /// Add all values in range as members of this set.
    pub(crate) fn insert_range(&mut self, range: RangeInclusive<u32>) {
        let start = *range.start();
        let end = *range.end();
        if start > end {
            return;
        }

        let major_start = self.get_major_value(start);
        let major_end = self.get_major_value(end);

        for major in major_start..=major_end {
            let page_start = start.max(self.major_start(major));
            let page_end = end.min(self.major_start(major + 1) - 1);
            let page = self.ensure_page_for_major_mut(major);
            page.insert_range(page_start, page_end);
        }
        self.mark_dirty();
    }

    /// An alternate version of extend() which is optimized for inserting an unsorted
    /// iterator of values.
    pub(crate) fn extend_unsorted<U: IntoIterator<Item = u32>>(&mut self, iter: U) {
        for val in iter {
            let major_value = self.get_major_value(val);
            let page = self.ensure_page_for_major_mut(major_value);
            page.insert_no_return(val);
        }
        self.mark_dirty();
    }

    /// Remove val from this set.
    pub(crate) fn remove(&mut self, val: u32) -> bool {
        let maybe_page = self.page_for_mut(val);
        if let Some(page) = maybe_page {
            let ret = page.remove(val);
            self.mark_dirty();
            ret
        } else {
            false
        }
    }

    // Remove all values in iter from this set.
    pub(crate) fn remove_all<U: IntoIterator<Item = u32>>(&mut self, iter: U) {
        let mut last_page_index: Option<usize> = None;
        let mut last_major_value = u32::MAX;
        for val in iter {
            let major_value = self.get_major_value(val);
            if major_value != last_major_value {
                last_page_index = self.page_index_for_major(major_value);
                last_major_value = major_value;
            };

            let Some(page_index) = last_page_index else {
                continue;
            };

            if let Some(page) = self.pages.get_mut(page_index) {
                page.remove(val);
            }
        }
        self.mark_dirty();
    }

    /// Removes all values in range as members of this set.
    pub(crate) fn remove_range(&mut self, range: RangeInclusive<u32>) {
        let start = *(range.start());
        let end = *(range.end());
        if start > end {
            return;
        }

        let major_start = self.get_major_value(start);
        let major_end = self.get_major_value(end);

        for major in major_start..=major_end {
            let page_start = start.max(self.major_start(major));
            let page_end = end.min(self.major_start(major + 1) - 1);
            if let Some(page) = self.page_for_major_mut(major) {
                page.remove_range(page_start, page_end);
            }
        }
        self.mark_dirty();
    }

    /// Returns true if val is a member of this set.
    pub(crate) fn contains(&self, val: u32) -> bool {
        self.page_for(val)
            .map(|page| page.contains(val))
            .unwrap_or(false)
    }

    pub(crate) fn empty() -> BitSet {
        BitSet {
            pages: vec![],
            page_map: vec![],
            len: Default::default(),
        }
    }

    /// Remove all members from this set.
    pub(crate) fn clear(&mut self) {
        self.pages.clear();
        self.page_map.clear();
        self.mark_dirty();
    }

    /// Return true if there are no members in this set.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of members in this set.
    pub(crate) fn len(&self) -> usize {
        // TODO(garretrieger): keep track of len on the fly, rather than computing it. Leave a computation method
        //                     for complex cases if needed.
        if self.is_dirty() {
            // this means we're stale and should recompute
            let len = self.pages.iter().map(|val| val.len()).sum();
            self.len.set(len);
        }
        self.len.get()
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = u32> + '_ {
        self.iter_non_empty_pages().flat_map(|(major, page)| {
            let base = self.major_start(major);
            page.iter().map(move |v| base + v)
        })
    }

    fn iter_pages(&self) -> impl DoubleEndedIterator<Item = (u32, &BitPage)> + '_ {
        self.page_map.iter().flat_map(|info| {
            self.pages
                .get(info.index as usize)
                .map(|page| (info.major_value, page))
        })
    }

    fn iter_non_empty_pages(&self) -> impl DoubleEndedIterator<Item = (u32, &BitPage)> + '_ {
        self.iter_pages().filter(|(_, page)| !page.is_empty())
    }

    fn mark_dirty(&mut self) {
        self.len.set(usize::MAX);
    }

    fn is_dirty(&self) -> bool {
        self.len.get() == usize::MAX
    }

    /// Return the major value (top 23 bits) of the page associated with value.
    fn get_major_value(&self, value: u32) -> u32 {
        value >> PAGE_BITS_LOG_2
    }

    fn major_start(&self, major: u32) -> u32 {
        major << PAGE_BITS_LOG_2
    }

    /// Returns the index in self.pages (if it exists) for the page with the same major as major_value.
    fn page_index_for_major(&self, major_value: u32) -> Option<usize> {
        self.page_map
            .binary_search_by(|probe| probe.major_value.cmp(&major_value))
            .ok()
            .map(|info_idx| self.page_map[info_idx].index as usize)
    }

    /// Returns the index in self.pages for the page with the same major as major_value. Will create the page
    /// if it does not yet exist.
    fn ensure_page_index_for_major(&mut self, major_value: u32) -> usize {
        match self
            .page_map
            .binary_search_by(|probe| probe.major_value.cmp(&major_value))
        {
            Ok(map_index) => self.page_map[map_index].index as usize,
            Err(map_index_to_insert) => {
                let page_index = self.pages.len();
                self.pages.push(BitPage::new_zeroes());
                let new_info = PageInfo {
                    index: page_index as u32,
                    major_value,
                };
                self.page_map.insert(map_index_to_insert, new_info);
                page_index
            }
        }
    }

    /// Return a reference to the page that 'value' resides in.
    fn page_for(&self, value: u32) -> Option<&BitPage> {
        let major_value = self.get_major_value(value);
        let pages_index = self.page_index_for_major(major_value)?;
        self.pages.get(pages_index)
    }

    /// Return a mutable reference to the page that 'value' resides in.
    ///
    /// Insert a new page if it doesn't exist.
    fn page_for_mut(&mut self, value: u32) -> Option<&mut BitPage> {
        let major_value = self.get_major_value(value);
        return self.page_for_major_mut(major_value);
    }

    // Return a mutable reference to the page with major value equal to major_value.
    fn page_for_major_mut(&mut self, major_value: u32) -> Option<&mut BitPage> {
        let page_index = self.page_index_for_major(major_value)?;
        self.pages.get_mut(page_index)
    }

    /// Return a mutable reference to the page that 'value' resides in.
    ///
    /// Insert a new page if it doesn't exist.
    fn ensure_page_for_mut(&mut self, value: u32) -> &mut BitPage {
        self.ensure_page_for_major_mut(self.get_major_value(value))
    }

    // Return a mutable reference to the page with major value equal to major_value.
    // Inserts a new page if it doesn't exist.
    fn ensure_page_for_major_mut(&mut self, major_value: u32) -> &mut BitPage {
        let page_index = self.ensure_page_index_for_major(major_value);
        self.pages.get_mut(page_index).unwrap()
    }
}

impl Extend<u32> for BitSet {
    fn extend<U: IntoIterator<Item = u32>>(&mut self, iter: U) {
        // TODO(garretrieger): additional optimization ideas:
        // - Assuming data is sorted accumulate a single element mask and only commit it to the element
        //   once the next value passes the end of the element.
        let mut last_page_index = usize::MAX;
        let mut last_major_value = u32::MAX;
        for val in iter {
            let major_value = self.get_major_value(val);
            if major_value != last_major_value {
                last_page_index = self.ensure_page_index_for_major(major_value);
                last_major_value = major_value;
            };
            if let Some(page) = self.pages.get_mut(last_page_index) {
                page.insert_no_return(val);
            }
        }
        self.mark_dirty();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PageInfo {
    // index into pages vector of this page
    index: u32,
    /// the top 23 bits of values covered by this page
    major_value: u32,
}

impl Hash for BitSet {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.iter_non_empty_pages().for_each(|t| t.hash(state));
    }
}

impl std::cmp::PartialEq for BitSet {
    fn eq(&self, other: &Self) -> bool {
        let mut this = self.iter_non_empty_pages();
        let mut other = other.iter_non_empty_pages();

        // Note: normally we would prefer to use zip, but we also
        //       need to check that both iters have the same length.
        loop {
            match (this.next(), other.next()) {
                (Some(a), Some(b)) if a == b => continue,
                (None, None) => return true,
                _ => return false,
            }
        }
    }
}

impl std::cmp::Eq for BitSet {}

impl std::cmp::Ord for PageInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major_value.cmp(&other.major_value)
    }
}

impl std::cmp::PartialOrd for PageInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn len() {
        let bitset = BitSet::empty();
        assert_eq!(bitset.len(), 0);
        assert!(bitset.is_empty());
    }

    #[test]
    fn iter() {
        let mut bitset = BitSet::empty();
        bitset.insert(3);
        bitset.insert(8);
        bitset.insert(534);
        bitset.insert(700);
        bitset.insert(10000);
        bitset.insert(10001);
        bitset.insert(10002);

        let v: Vec<u32> = bitset.iter().collect();
        assert_eq!(v, vec![3, 8, 534, 700, 10000, 10001, 10002]);
    }

    #[test]
    fn iter_backwards() {
        let mut bitset = BitSet::empty();

        bitset.insert_range(1..=6);
        {
            let mut it = bitset.iter();
            assert_eq!(Some(1), it.next());
            assert_eq!(Some(6), it.next_back());
            assert_eq!(Some(5), it.next_back());
            assert_eq!(Some(2), it.next());
            assert_eq!(Some(3), it.next());
            assert_eq!(Some(4), it.next());
            assert_eq!(None, it.next());
            assert_eq!(None, it.next_back());
        }

        bitset.insert_range(700..=701);
        {
            let mut it = bitset.iter();
            assert_eq!(Some(1), it.next());
            assert_eq!(Some(701), it.next_back());
            assert_eq!(Some(700), it.next_back());
            assert_eq!(Some(6), it.next_back());
            assert_eq!(Some(5), it.next_back());
            assert_eq!(Some(2), it.next());
            assert_eq!(Some(3), it.next());
            assert_eq!(Some(4), it.next());
            assert_eq!(None, it.next());
            assert_eq!(None, it.next_back());
        }

        let v: Vec<u32> = bitset.iter().rev().collect();
        assert_eq!(vec![701, 700, 6, 5, 4, 3, 2, 1], v);
    }

    #[test]
    fn extend() {
        let values = [3, 8, 534, 700, 10000, 10001, 10002];
        let values_unsorted = [10000, 3, 534, 700, 8, 10001, 10002];

        let mut s1 = BitSet::empty();
        let mut s2 = BitSet::empty();
        let mut s3 = BitSet::empty();
        let mut s4 = BitSet::empty();
        assert_eq!(s1.len(), 0);

        s1.extend(values.iter().copied());
        s2.extend_unsorted(values.iter().copied());
        s3.extend(values_unsorted.iter().copied());
        s4.extend_unsorted(values_unsorted.iter().copied());

        assert_eq!(s1.iter().collect::<Vec<u32>>(), values);
        assert_eq!(s2.iter().collect::<Vec<u32>>(), values);
        assert_eq!(s3.iter().collect::<Vec<u32>>(), values);
        assert_eq!(s4.iter().collect::<Vec<u32>>(), values);

        assert_eq!(s1.len(), 7);
        assert_eq!(s2.len(), 7);
        assert_eq!(s3.len(), 7);
        assert_eq!(s4.len(), 7);
    }

    #[test]
    fn insert_unordered() {
        let mut bitset = BitSet::empty();

        assert!(!bitset.contains(0));
        assert!(!bitset.contains(768));
        assert!(!bitset.contains(1678));

        assert!(bitset.insert(0));
        assert!(bitset.insert(1678));
        assert!(bitset.insert(768));

        assert!(bitset.contains(0));
        assert!(bitset.contains(768));
        assert!(bitset.contains(1678));

        assert!(!bitset.contains(1));
        assert!(!bitset.contains(769));
        assert!(!bitset.contains(1679));

        assert_eq!(bitset.len(), 3);
    }

    #[test]
    fn remove() {
        let mut bitset = BitSet::empty();

        assert!(bitset.insert(0));
        assert!(bitset.insert(511));
        assert!(bitset.insert(512));
        assert!(bitset.insert(1678));
        assert!(bitset.insert(768));

        assert_eq!(bitset.len(), 5);

        assert!(!bitset.remove(12));
        assert!(bitset.remove(511));
        assert!(bitset.remove(512));
        assert!(!bitset.remove(512));

        assert_eq!(bitset.len(), 3);
        assert!(bitset.contains(0));
        assert!(!bitset.contains(511));
        assert!(!bitset.contains(512));
    }

    #[test]
    fn remove_all() {
        let mut bitset = BitSet::empty();
        bitset.extend([5, 7, 11, 18, 620, 2000]);

        assert_eq!(bitset.len(), 6);

        bitset.remove_all([7, 11, 13, 18, 620]);
        assert_eq!(bitset.len(), 2);
        assert_eq!(bitset.iter().collect::<Vec<u32>>(), vec![5, 2000]);
    }

    #[test]
    fn remove_range() {
        let mut bitset = BitSet::empty();
        bitset.extend([5, 7, 11, 18, 620, 2000]);

        assert_eq!(bitset.len(), 6);

        bitset.remove_range(7..=620);
        assert_eq!(bitset.len(), 2);
        assert_eq!(bitset.iter().collect::<Vec<u32>>(), vec![5, 2000]);
    }

    #[test]
    fn remove_to_empty_page() {
        let mut bitset = BitSet::empty();

        bitset.insert(793);
        bitset.insert(43);
        bitset.remove(793);

        assert!(bitset.contains(43));
        assert!(!bitset.contains(793));
        assert_eq!(bitset.len(), 1);
    }

    #[test]
    fn insert_max_value() {
        let mut bitset = BitSet::empty();
        assert!(!bitset.contains(u32::MAX));
        assert!(bitset.insert(u32::MAX));
        assert!(bitset.contains(u32::MAX));
        assert!(!bitset.contains(u32::MAX - 1));
        assert_eq!(bitset.len(), 1);
    }

    fn set_for_range(first: u32, last: u32) -> BitSet {
        let mut set = BitSet::empty();
        for i in first..=last {
            set.insert(i);
        }
        set
    }

    #[test]
    fn insert_range() {
        for range in [
            (0, 0),
            (0, 364),
            (0, 511),
            (512, 1023),
            (0, 1023),
            (364, 700),
            (364, 6000),
        ] {
            let mut set = BitSet::empty();
            set.len();
            set.insert_range(range.0..=range.1);
            assert_eq!(set, set_for_range(range.0, range.1), "{range:?}");
            assert_eq!(set.len(), (range.1 - range.0 + 1) as usize, "{range:?}");
        }
    }

    #[test]
    fn insert_range_on_existing() {
        let mut set = BitSet::empty();
        set.insert(700);
        set.insert(2000);
        set.insert_range(32..=4000);
        assert_eq!(set, set_for_range(32, 4000));
        assert_eq!(set.len(), 4000 - 32 + 1);
    }

    #[test]
    fn clear() {
        let mut bitset = BitSet::empty();

        bitset.insert(13);
        bitset.insert(670);
        assert!(bitset.contains(13));
        assert!(bitset.contains(670));

        bitset.clear();
        assert!(!bitset.contains(13));
        assert!(!bitset.contains(670));
        assert_eq!(bitset.len(), 0);
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn hash_and_eq() {
        let mut bitset1 = BitSet::empty();
        let mut bitset2 = BitSet::empty();
        let mut bitset3 = BitSet::empty();
        let mut bitset4 = BitSet::empty();

        bitset1.insert(43);
        bitset1.insert(793);

        bitset2.insert(793);
        bitset2.insert(43);
        bitset2.len();

        bitset3.insert(43);
        bitset3.insert(793);
        bitset3.insert(794);

        bitset4.insert(0);

        assert_eq!(BitSet::empty(), BitSet::empty());
        assert_eq!(bitset1, bitset2);
        assert_ne!(bitset1, bitset3);
        assert_ne!(bitset2, bitset3);
        assert_eq!(bitset4, bitset4);

        let set = HashSet::from([bitset1]);
        assert!(set.contains(&bitset2));
        assert!(!set.contains(&bitset3));
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn hash_and_eq_with_empty_pages() {
        let mut bitset1 = BitSet::empty();
        let mut bitset2 = BitSet::empty();
        let mut bitset3 = BitSet::empty();

        bitset1.insert(43);

        bitset2.insert(793);
        bitset2.insert(43);
        bitset2.remove(793);

        bitset3.insert(43);
        bitset3.insert(793);

        assert_eq!(bitset1, bitset2);
        assert_ne!(bitset1, bitset3);

        let set = HashSet::from([bitset1]);
        assert!(set.contains(&bitset2));
    }
}
