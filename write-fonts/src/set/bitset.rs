//! A fast & efficient integer bitset that keeps it's members ordered.

use super::bitpage::BitPage;
use std::cell::Cell;

// log_2(PAGE_BITS)
const PAGE_BITS_LOG_2: u32 = 9; // 512 bits, TODO(garretrieger): compute?

/// An ordered integer set
#[derive(Clone, Debug, Default)]
pub struct BitSet<T> {
    // TODO(garretrieger): consider a "small array" type instead of Vec.
    pages: Vec<BitPage>,
    page_map: Vec<PageInfo>,
    len: Cell<usize>, // TODO(garretrieger): use an option instead of a sentinel.
    phantom: std::marker::PhantomData<T>,
}

impl<T: Into<u32>> BitSet<T> {
    fn insert(&mut self, val: T) -> bool {
        let val = val.into();
        let page = self.page_for_mut(val);
        let ret = page.insert(val);
        self.mark_dirty();
        ret
    }

    fn contains(&self, val: T) -> bool {
        let val = val.into();
        self.page_for(val)
            .map(|page| page.contains(val))
            .unwrap_or(false)
    }

    fn len(&self) -> usize {
        if self.is_dirty() {
            // this means we're stale and should recompute
            let len = self.pages.iter().map(|val| val.len()).sum();
            self.len.set(len);
        }
        self.len.get()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn mark_dirty(&mut self) {
        self.len.set(usize::MAX);
    }

    fn is_dirty(&self) -> bool {
        self.len.get() == usize::MAX
    }

    /// Return the major value (top 23 bits) of the page associated with value.
    fn get_major_value(&self, value: u32) -> u32 {
        return value >> PAGE_BITS_LOG_2;
    }

    /// Return a reference to the that 'value' resides in.
    fn page_for(&self, value: u32) -> Option<&BitPage> {
        let major_value = self.get_major_value(value);
        self.page_map
            .binary_search_by(|probe| probe.major_value.cmp(&major_value))
            .ok()
            .and_then(|info_idx| {
                let real_idx = self.page_map[info_idx].index as usize;
                self.pages.get(real_idx)
            })
    }

    /// Return a mutable reference to the that 'value' resides in. Insert a new
    /// page if it doesn't exist.
    fn page_for_mut(&mut self, value: u32) -> &mut BitPage {
        let major_value = self.get_major_value(value);
        match self
            .page_map
            .binary_search_by(|probe| probe.major_value.cmp(&major_value))
        {
            Ok(idx) => self.pages.get_mut(idx).unwrap(),
            Err(idx_to_insert) => {
                let index = self.pages.len() as u32;
                self.pages.push(BitPage::new_zeroes());
                let new_info = PageInfo { index, major_value };
                self.page_map.insert(idx_to_insert, new_info);
                self.pages.last_mut().unwrap()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PageInfo {
    // index into pages vector of this page
    index: u32,
    /// the top 23 bits of values covered by this page
    major_value: u32,
}

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

    #[test]
    fn bitset_len() {
        let bitset = BitSet::<u32>::default();
        assert_eq!(bitset.len(), 0);
        assert!(bitset.is_empty());
    }

    #[test]
    fn bitset_insert_unordered() {
        let mut bitset = BitSet::<u32>::default();

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
    fn bitset_insert_max_value() {
        let mut bitset = BitSet::<u32>::default();
        assert!(!bitset.contains(u32::MAX));
        assert!(bitset.insert(u32::MAX));
        assert!(bitset.contains(u32::MAX));
        assert!(!bitset.contains(u32::MAX - 1));
        assert_eq!(bitset.len(), 1);
    }
}
