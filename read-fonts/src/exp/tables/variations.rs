//! `variations`, generated into the reworked framework.

use super::super::prelude::*;

include!("../../../generated/exp/generated_variations.rs");

// Helpers the generated `#[count(..)]` expressions call. Copied from the
// hand-written half of `read-fonts/src/tables/variations.rs`; they are pure
// arithmetic over generated types and do not depend on how anything is parsed.

impl EntryFormat {
    pub fn entry_size(self) -> u8 {
        ((self.bits() & Self::MAP_ENTRY_SIZE_MASK.bits()) >> 4) + 1
    }

    pub fn bit_count(self) -> u8 {
        (self.bits() & Self::INNER_INDEX_BIT_COUNT_MASK.bits()) + 1
    }

    pub(crate) fn map_size(self, map_count: impl Into<u32>) -> usize {
        self.entry_size() as usize * map_count.into() as usize
    }
}

impl ItemVariationData<'_> {
    /// The length of one delta set.
    pub fn delta_row_len(word_delta_count: u16, region_index_count: u16) -> usize {
        let region_count = region_index_count as usize;
        let long_words = word_delta_count & 0x8000 != 0;
        let (word_size, small_size) = if long_words { (4, 2) } else { (2, 1) };
        let long_delta_count = (word_delta_count & 0x7FFF) as usize;
        let short_delta_count = region_count.saturating_sub(long_delta_count);
        long_delta_count * word_size + short_delta_count * small_size
    }

    /// The length in bytes of the delta sets data.
    pub fn delta_sets_len(
        item_count: u16,
        word_delta_count: u16,
        region_index_count: u16,
    ) -> usize {
        Self::delta_row_len(word_delta_count, region_index_count) * item_count as usize
    }
}

// `extern scalar TupleIndex`: a plain scalar wrapper, identical whichever
// framework reads it, so the existing one is reused.
pub use crate::tables::variations::TupleIndex;
