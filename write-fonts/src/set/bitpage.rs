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

    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.storage
            .iter()
            .enumerate()
            .filter(|(_, elem)| **elem != 0)
            .flat_map(|(i, elem)| {
                iter_bit_indices(*elem).map(move |idx| (i as u32 * ELEM_BITS) + idx)
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
        // TODO(garretrieger): there may be a more efficient way to do this with rust std lib.
        //   Look for a way to do something like bitscanforward/__builtin_ctz that harfbuzz uses.
        while idx < ELEM_BITS {
            let mask = 1 << idx;
            idx += 1;
            if (val & mask) != 0 {
                return Some(idx - 1);
            }
        }
        None
    })
}

#[cfg(test)]
mod test {
    use super::*;

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
        page.insert(511);
        page.insert(23);
        page.insert(400);
        page.insert(78);

        let items: Vec<_> = page.iter().collect();
        assert_eq!(items, vec![0, 12, 23, 78, 400, 511,])
    }
}
