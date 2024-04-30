//! Stores a page of bits, used inside of bitset's.

use std::cell::Cell;

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
pub const PAGE_BITS: u32 = ELEM_BITS * PAGE_SIZE;
// mask out the bits of a value not used to index into a page
const PAGE_MASK: u32 = PAGE_BITS - 1;

#[derive(Clone)]
pub struct BitPage {
    storage: [Element; PAGE_SIZE as usize],
    len: Cell<u32>,
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

impl BitPage {
    pub fn new_zeroes() -> Self {
        Self {
            storage: [0; PAGE_SIZE as usize],
            len: Cell::new(0),
        }
    }

    pub fn new_ones() -> Self {
        Self {
            storage: [Element::MAX; PAGE_SIZE as usize],
            len: Cell::new(PAGE_SIZE * ELEM_BITS),
        }
    }

    pub fn len(&self) -> usize {
        if self.is_dirty() {
            // this means we're stale and should recompute
            let len = self.storage.iter().map(|val| val.count_ones()).sum();
            self.len.set(len);
        }
        self.len.get() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // TODO(garretrieger): iterator that starts after some value (similar to next in hb).
    // TODO(garretrieger): reverse iterator.

    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.storage
            .iter()
            .enumerate()
            .filter(|(_, elem)| **elem != 0)
            .flat_map(|(i, elem)| {
                let base = i as u32 * ELEM_BITS;
                iter_bit_indices(*elem).map(move |idx| base + idx)
            })
    }

    pub fn insert(&mut self, val: u32) -> bool {
        let ret = !self.contains(val);
        *self.element_mut(val) |= elem_index_bit_mask(val);
        self.mark_dirty();
        ret
    }

    pub fn remove(&mut self, val: u32) -> bool {
        let ret = self.contains(val);
        *self.element_mut(val) &= !elem_index_bit_mask(val);
        self.mark_dirty();
        ret
    }

    pub fn contains(&self, val: u32) -> bool {
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
        let mask = 1u64.checked_shl(idx).unwrap_or(0) - 1;
        let masked = val & !mask;
        let next_index = masked.trailing_zeros();
        if next_index >= ELEM_BITS {
            return None;
        }
        idx = next_index + 1;
        Some(next_index)
    })
}

#[cfg(test)]
mod test {
    use super::*;

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
}
