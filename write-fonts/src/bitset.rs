//! A fast & efficient integer bitset that keeps it's members ordered.

use std::cell::Cell;

/// An ordered integer set
#[derive(Clone, Debug, Default)]
pub struct Bitset<T> {
    // TODO(garretrieger): consider a "small array" type instead of Vec.
    pages: Vec<Page>,
    page_map: Vec<PageInfo>,
    len: Cell<usize>, // TODO(garretrieger): use an option instead of a sentinel.
    phantom: std::marker::PhantomData<T>,
}

impl<T: Into<u32>> Bitset<T> {
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

    /// Return the first value of the page associated with value.
    fn get_first_value(&self, value: u32) -> u32 {
        return value >> PAGE_BITS_LOG_2;
    }

    /// Return a reference to the that 'value' resides in.
    fn page_for(&self, value: u32) -> Option<&Page> {
        let first_value = self.get_first_value(value);
        self.page_map
            .binary_search_by(|probe| probe.first_value.cmp(&first_value))
            .ok()
            .and_then(|info_idx| {
                let real_idx = self.page_map[info_idx].index as usize;
                self.pages.get(real_idx)
            })
    }

    /// Return a mutable reference to the that 'value' resides in. Insert a new
    /// page if it doesn't exist.
    fn page_for_mut(&mut self, value: u32) -> &mut Page {
        let first_value = self.get_first_value(value);
        match self
            .page_map
            .binary_search_by(|probe| probe.first_value.cmp(&first_value))
        {
            Ok(idx) => self.pages.get_mut(idx).unwrap(),
            Err(idx_to_insert) => {
                let index = self.pages.len() as u32;
                self.pages.push(Page::new_zeroes());
                let new_info = PageInfo { index, first_value };
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
    /// the first value covered by this page
    first_value: u32,
}

impl std::cmp::Ord for PageInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.first_value.cmp(&other.first_value)
    }
}

impl std::cmp::PartialOrd for PageInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

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
const PAGE_BITS: u32 = ELEM_BITS * PAGE_SIZE;
// log_2(PAGE_BITS)
const PAGE_BITS_LOG_2: u32 = 9; // 512 bits, TODO(garretrieger): compute?
                                // mask out the bits of a value not used to index into a page
const PAGE_MASK: u32 = PAGE_BITS - 1;

#[derive(Clone)]
struct Page {
    storage: [Element; PAGE_SIZE as usize],
    len: Cell<u32>,
}

impl Default for Page {
    fn default() -> Self {
        Self::new_zeroes()
    }
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let values: Vec<_> = self.iter().collect();
        std::fmt::Debug::fmt(&values, f)
    }
}

impl Page {
    fn new_zeroes() -> Self {
        Self {
            storage: [0; PAGE_SIZE as usize],
            len: Cell::new(0),
        }
    }

    fn new_ones() -> Self {
        Self {
            storage: [Element::MAX; PAGE_SIZE as usize],
            len: Cell::new(PAGE_SIZE * ELEM_BITS),
        }
    }

    fn mark_dirty(&mut self) {
        self.len.set(u32::MAX);
    }

    fn is_dirty(&self) -> bool {
        self.len.get() == u32::MAX
    }

    fn len(&self) -> usize {
        if self.is_dirty() {
            // this means we're stale and should recompute
            let len = self.storage.iter().map(|val| val.count_ones()).sum();
            self.len.set(len);
        }
        self.len.get() as usize
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.storage
            .iter()
            .enumerate()
            .filter(|(_, elem)| **elem != 0)
            .flat_map(|(i, elem)| {
                iter_bit_indices(*elem).map(move |idx| (i as u32 * ELEM_BITS) + idx)
            })
    }

    fn insert(&mut self, val: u32) -> bool {
        let ret = !self.contains(val);
        *self.element_mut(val) |= elem_index_bit_mask(val);
        self.mark_dirty();
        ret
    }

    fn remove(&mut self, val: u32) -> bool {
        let ret = self.contains(val);
        *self.element_mut(val) &= !elem_index_bit_mask(val);
        self.mark_dirty();
        ret
    }

    fn contains(&self, val: u32) -> bool {
        (*self.element(val) & elem_index_bit_mask(val)) != 0
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
    fn bitset_len() {
        let bitset = Bitset::<u32>::default();
        assert_eq!(bitset.len(), 0);
        assert!(bitset.is_empty());
    }

    #[test]
    fn bitset_insert_unordered() {
        let mut bitset = Bitset::<u32>::default();

        assert!(!bitset.contains(0));
        assert!(!bitset.contains(768));
        assert!(!bitset.contains(1678));

        assert!(bitset.insert(0));
        assert!(bitset.insert(1678));
        assert!(bitset.insert(768));

        //   eprintln!("{bitset:?")
        dbg!(&bitset);
        assert!(bitset.contains(0));
        assert!(bitset.contains(768));
        assert!(bitset.contains(1678));

        assert!(!bitset.contains(1));
        assert!(!bitset.contains(769));
        assert!(!bitset.contains(1679));

        assert_eq!(bitset.len(), 3);
    }

    #[test]
    fn sanity_check_page_init() {
        let page = Page::new_zeroes();
        assert_eq!(page.len(), 0);
        assert!(page.is_empty());
    }

    #[test]
    fn sanity_check_page_init_ones() {
        let page = Page::new_ones();
        assert_eq!(page.len(), 512);
        assert!(!page.is_empty());
    }

    #[test]
    fn page_contains_empty() {
        let page = Page::new_zeroes();
        assert!(!page.contains(0));
        assert!(!page.contains(1));
        assert!(!page.contains(75475));
    }

    #[test]
    fn page_contains_all() {
        let page = Page::new_ones();
        assert!(page.contains(0));
        assert!(page.contains(1));
        assert!(page.contains(75475));
    }

    #[test]
    fn page_insert() {
        for val in 0..=1025 {
            let mut page = Page::new_zeroes();
            assert!(!page.contains(val), "unexpected {val} (1)");
            page.insert(val);
            assert!(page.contains(val), "missing {val}");
            assert!(!page.contains(val.wrapping_sub(1)), "unexpected {val} (2)");
        }
    }

    #[test]
    fn page_insert_return() {
        let mut page = Page::new_zeroes();
        assert!(page.insert(123));
        assert!(!page.insert(123));
    }

    #[test]
    fn page_remove() {
        for val in 0..=1025 {
            let mut page = Page::new_ones();
            assert!(page.contains(val), "missing {val} (1)");
            assert!(page.remove(val));
            assert!(!page.remove(val));
            assert!(!page.contains(val), "unexpected {val}");
            assert!(page.contains(val.wrapping_sub(1)), "missing {val} (2)");
        }
    }

    #[test]
    fn page_iter() {
        let mut page = Page::new_zeroes();

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
