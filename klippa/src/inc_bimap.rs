//! inc_bimap: incremental bijective map, only lhs is given, rhs is incrementally assigned
//! ported from Harfbuzz hb_inc_bimap_t: <https://github.com/harfbuzz/harfbuzz/blob/b5a65e0f20c30a7f13b2f6619479a6d666e603e0/src/hb-bimap.hh#L97>

use fnv::FnvHashMap;

#[derive(Default)]
pub(crate) struct IncBiMap {
    forw_map: FnvHashMap<u32, u32>,
    back_map: Vec<u32>,
}

impl IncBiMap {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            forw_map: FnvHashMap::with_capacity_and_hasher(capacity, Default::default()),
            back_map: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.forw_map.clear();
        self.back_map.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.forw_map.len()
    }

    pub(crate) fn add(&mut self, lhs: u32) -> u32 {
        match self.forw_map.get(&lhs) {
            Some(&rhs) => rhs,
            None => {
                let rhs = self.back_map.len() as u32;
                self.forw_map.insert(lhs, rhs);
                self.back_map.push(lhs);
                rhs
            }
        }
    }
}
