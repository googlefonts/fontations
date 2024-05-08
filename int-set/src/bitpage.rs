//! Stores a page of bits, used inside of bitset's.

use std::{cell::Cell, hash::Hash};

// the integer type underlying our bit set
type Element = u64;

// the number of elements in a page
const PAGE_SIZE: u32 = 8;
// the length of an element in bytes
const ELEM_SIZE: u32 = std::mem::size_of::<Element>() as u32;
// the length of an element in bits
const ELEM_BITS: u32 = ELEM_SIZE * 8;
// mask out bits of a value not used to index into an element
const ELEM_MASK: u32 = ELEM_BITS - 1;
// the number of bits in a page
pub(crate) const PAGE_BITS: u32 = ELEM_BITS * PAGE_SIZE;
// mask out the bits of a value not used to index into a page
const PAGE_MASK: u32 = PAGE_BITS - 1;

/// A fixed size (512 bits wide) page of bits that records integer set membership from [0, 511].
#[derive(Clone)]
pub(crate) struct BitPage {
    storage: [Element; PAGE_SIZE as usize],
    len: Cell<u32>,
}

impl BitPage {
    /// Create a new page with no bits set.
    pub(crate) fn new_zeroes() -> Self {
        Self {
            storage: [0; PAGE_SIZE as usize],
            len: Cell::new(0),
        }
    }

    /// Returns the number of members in this page.
    pub(crate) fn len(&self) -> usize {
        if self.is_dirty() {
            // this means we're stale and should recompute
            let len = self.storage.iter().map(|val| val.count_ones()).sum();
            self.len.set(len);
        }
        self.len.get() as usize
    }

    /// Returns true if this page has no members.
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // TODO(garretrieger): iterator that starts after some value (similar to next in hb).
    // TODO(garretrieger): reverse iterator.

    /// Iterator over the members of this page.
    pub(crate) fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.storage
            .iter()
            .enumerate()
            .filter(|(_, elem)| **elem != 0)
            .flat_map(|(i, elem)| {
                let base = i as u32 * ELEM_BITS;
                iter_bit_indices(*elem).map(move |idx| base + idx)
            })
    }

    /// Marks (val % page width) a member of this set and returns true if it is newly added.
    pub(crate) fn insert(&mut self, val: u32) -> bool {
        let ret = !self.contains(val);
        *self.element_mut(val) |= elem_index_bit_mask(val);
        self.mark_dirty();
        ret
    }

    /// Marks (val % page width) a member of this set, but does not check if it was already a member.
    ///
    /// This is used to maximize performance in cases where the return value on insert() is not needed.
    pub(crate) fn insert_no_return(&mut self, val: u32) {
        *self.element_mut(val) |= elem_index_bit_mask(val);
        self.mark_dirty();
    }

    /// Marks all values [first, last] as members of this set.
    pub(crate) fn insert_range(&mut self, first: u32, last: u32) {
        let first = first & PAGE_MASK;
        let last = last & PAGE_MASK;
        let first_elem_idx = first / ELEM_BITS;
        let last_elem_idx = last / ELEM_BITS;

        for elem_idx in first_elem_idx..=last_elem_idx {
            let elem_start = first.max(elem_idx * ELEM_BITS) & ELEM_MASK;
            let elem_last = last.min(((elem_idx + 1) * ELEM_BITS) - 1) & ELEM_MASK;

            let end_shift = ELEM_BITS - elem_last - 1;
            let mask = u64::MAX << (elem_start + end_shift);
            let mask = mask >> end_shift;

            self.storage[elem_idx as usize] |= mask;
        }

        self.mark_dirty();
    }

    /// Removes (val % page width) from this set.
    pub(crate) fn remove(&mut self, val: u32) -> bool {
        let ret = self.contains(val);
        *self.element_mut(val) &= !elem_index_bit_mask(val);
        self.mark_dirty();
        ret
    }

    /// Return true if (val % page width) is a member of this set.
    pub(crate) fn contains(&self, val: u32) -> bool {
        (*self.element(val) & elem_index_bit_mask(val)) != 0
    }

    fn mark_dirty(&mut self) {
        self.len.set(u32::MAX);
    }

    fn is_dirty(&self) -> bool {
        self.len.get() == u32::MAX
    }

    fn element(&self, value: u32) -> &Element {
        let idx = (value & PAGE_MASK) / ELEM_BITS;
        &self.storage[idx as usize]
    }

    fn element_mut(&mut self, value: u32) -> &mut Element {
        let idx = (value & PAGE_MASK) / ELEM_BITS;
        &mut self.storage[idx as usize]
    }
}

/// returns the bit to set in an element for this value
const fn elem_index_bit_mask(value: u32) -> Element {
    1 << (value & ELEM_MASK)
}

fn iter_bit_indices(val: Element) -> impl Iterator<Item = u32> {
    let mut idx = 0;

    std::iter::from_fn(move || {
        if idx >= ELEM_BITS {
            return None;
        }
        let mask = (1u64 << idx) - 1;
        let masked = val & !mask;
        let next_index = masked.trailing_zeros();
        if next_index >= ELEM_BITS {
            return None;
        }
        idx = next_index + 1;
        Some(next_index)
    })
}

impl Default for BitPage {
    fn default() -> Self {
        Self::new_zeroes()
    }
}

impl std::fmt::Debug for BitPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let values: Vec<_> = self.iter().collect();
        std::fmt::Debug::fmt(&values, f)
    }
}

impl Hash for BitPage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.storage.hash(state);
    }
}

impl std::cmp::PartialEq for BitPage {
    fn eq(&self, other: &Self) -> bool {
        self.storage == other.storage
    }
}

impl std::cmp::Eq for BitPage {}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use super::*;

    impl BitPage {
        /// Create a new page with all bits set.
        pub(crate) fn new_ones() -> Self {
            Self {
                storage: [Element::MAX; PAGE_SIZE as usize],
                len: Cell::new(PAGE_SIZE * ELEM_BITS),
            }
        }
    }

    #[test]
    fn test_iter_bit_indices() {
        let items: Vec<_> = iter_bit_indices(0).collect();
        assert_eq!(items, vec![]);

        let items: Vec<_> = iter_bit_indices(1).collect();
        assert_eq!(items, vec![0]);

        let items: Vec<_> = iter_bit_indices(0b1100).collect();
        assert_eq!(items, vec![2, 3]);

        let items: Vec<_> = iter_bit_indices(1 << 63).collect();
        assert_eq!(items, vec![63]);

        let items: Vec<_> = iter_bit_indices((1 << 47) | (1 << 63)).collect();
        assert_eq!(items, vec![47, 63]);
    }

    #[test]
    fn page_init() {
        let page = BitPage::new_zeroes();
        assert_eq!(page.len(), 0);
        assert!(page.is_empty());
    }

    #[test]
    fn page_init_ones() {
        let page = BitPage::new_ones();
        assert_eq!(page.len(), 512);
        assert!(!page.is_empty());
    }

    #[test]
    fn page_contains_empty() {
        let page = BitPage::new_zeroes();
        assert!(!page.contains(0));
        assert!(!page.contains(1));
        assert!(!page.contains(75475));
    }

    #[test]
    fn page_contains_all() {
        let page = BitPage::new_ones();
        assert!(page.contains(0));
        assert!(page.contains(1));
        assert!(page.contains(75475));
    }

    #[test]
    fn page_insert() {
        for val in 0..=1025 {
            let mut page = BitPage::new_zeroes();
            assert!(!page.contains(val), "unexpected {val} (1)");
            page.insert(val);
            assert!(page.contains(val), "missing {val}");
            assert!(!page.contains(val.wrapping_sub(1)), "unexpected {val} (2)");
        }
    }

    #[test]
    fn page_insert_range() {
        fn page_for_range(first: u32, last: u32) -> BitPage {
            let mut page = BitPage::new_zeroes();
            for i in first..=last {
                page.insert(i);
            }
            page
        }

        for range in [
            (0, 0),
            (0, 1),
            (1, 15),
            (5, 63),
            (64, 67),
            (69, 72),
            (69, 127),
            (32, 345),
            (0, 511),
        ] {
            let mut page = BitPage::new_zeroes();
            page.insert_range(range.0, range.1);
            assert_eq!(page, page_for_range(range.0, range.1), "{range:?}");
        }
    }

    #[test]
    fn page_insert_return() {
        let mut page = BitPage::new_zeroes();
        assert!(page.insert(123));
        assert!(!page.insert(123));
    }

    #[test]
    fn page_remove() {
        for val in 0..=1025 {
            let mut page = BitPage::new_ones();
            assert!(page.contains(val), "missing {val} (1)");
            assert!(page.remove(val));
            assert!(!page.remove(val));
            assert!(!page.contains(val), "unexpected {val}");
            assert!(page.contains(val.wrapping_sub(1)), "missing {val} (2)");
        }
    }

    #[test]
    fn remove_to_empty_page() {
        let mut page = BitPage::new_zeroes();

        page.insert(13);
        assert!(!page.is_empty());

        page.remove(13);
        assert!(page.is_empty());
    }

    #[test]
    fn page_iter() {
        let mut page = BitPage::new_zeroes();

        page.insert(0);
        page.insert(12);
        page.insert(13);
        page.insert(63);
        page.insert(64);
        page.insert(511);
        page.insert(23);
        page.insert(400);
        page.insert(78);

        let items: Vec<_> = page.iter().collect();
        assert_eq!(items, vec![0, 12, 13, 23, 63, 64, 78, 400, 511,])
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn hash_and_eq() {
        let mut page1 = BitPage::new_zeroes();
        let mut page2 = BitPage::new_zeroes();
        let mut page3 = BitPage::new_zeroes();

        page1.insert(12);
        page1.insert(300);

        page2.insert(300);
        page2.insert(12);
        page2.len();

        page3.insert(300);
        page3.insert(12);
        page3.insert(23);

        assert_eq!(page1, page2);
        assert_ne!(page1, page3);
        assert_ne!(page2, page3);

        let set = HashSet::from([page1]);
        assert!(set.contains(&page2));
        assert!(!set.contains(&page3));
    }
}
