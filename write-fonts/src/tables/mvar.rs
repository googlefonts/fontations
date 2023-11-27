//! The [MVAR](https://learn.microsoft.com/en-us/typography/opentype/spec/mvar) table

include!("../../generated/generated_mvar.rs");

use super::variations::ItemVariationStore;
use std::mem::size_of;

impl Mvar {
    /// Construct a new `MVAR` table.
    pub fn new(
        version: MajorMinor,
        item_variation_store: Option<ItemVariationStore>,
        value_records: Vec<ValueRecord>,
    ) -> Self {
        Self {
            version,
            value_record_size: size_of::<ValueRecord>() as u16,
            value_record_count: value_records.len() as u16,
            item_variation_store: item_variation_store.into(),
            value_records: value_records.into_iter().map(Into::into).collect(),
        }
    }
}
