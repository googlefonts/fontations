//! Access to the lookup-oriented feature variations added in ISO OFF fifth edition.

use super::{FeatureLookups, FeatureVariations, LookupVariationRecord};
use crate::{types::Compatible, ReadError};

impl<'a> FeatureVariations<'a> {
    /// Lookup variations, sorted by feature index; absent in version 1.0.
    pub fn lookup_variation_records(&self) -> Option<&'a [LookupVariationRecord]> {
        if !self.version().compatible((1, 1)) {
            return None;
        }
        // A present, empty version-1.1 array is distinct from an absent field.
        self.lookup_variation_record_count()?;
        self.offset_data()
            .read_array(self.lookup_variation_records_byte_range())
            .ok()
    }

    /// Returns the conditional lookup table for a feature, if present.
    ///
    /// The table's lookups replace the current feature's lookups unless
    /// [`super::FeatureLookupsFlags::ADD_DEFAULT_LOOKUPS`] is set. The current
    /// feature may be an alternate selected by a feature variation record;
    /// its feature parameters are unaffected by lookup variations.
    pub fn feature_lookups(
        &self,
        feature_index: u16,
    ) -> Option<Result<FeatureLookups<'a>, ReadError>> {
        let records = self.lookup_variation_records()?;
        let index = records
            .binary_search_by_key(&feature_index, LookupVariationRecord::feature_index)
            .ok()?;
        Some(records[index].feature_lookups(self.offset_data()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{types::MajorMinor, FontRead};

    #[test]
    fn lookup_variations_versioned_empty_arrays() {
        let v1_0 = [0, 1, 0, 0, 0, 0, 0, 0];
        let table = FeatureVariations::read(v1_0.as_slice().into()).unwrap();
        assert_eq!(table.version(), MajorMinor::VERSION_1_0);
        assert_eq!(table.lookup_variation_record_count(), None);
        assert!(table.lookup_variation_records().is_none());
        assert!(table.feature_lookups(0).is_none());

        let v1_1 = [0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        let table = FeatureVariations::read(v1_1.as_slice().into()).unwrap();
        assert_eq!(table.version(), MajorMinor::VERSION_1_1);
        assert_eq!(table.lookup_variation_record_count(), Some(0));
        assert_eq!(table.lookup_variation_records().unwrap().len(), 0);
        assert!(table.feature_lookups(0).is_none());
    }

    #[test]
    fn lookup_variations_truncated_arrays_and_invalid_offsets() {
        let bytes = [
            0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 3, 0xff, 0xff, 0xff, 0xff,
        ];
        for len in 8..bytes.len() {
            let table = FeatureVariations::read(bytes[..len].into()).unwrap();
            assert!(table.lookup_variation_records().is_none());
            assert!(table.feature_lookups(3).is_none());
        }
        let table = FeatureVariations::read(bytes.as_slice().into()).unwrap();
        assert_eq!(table.lookup_variation_records().unwrap().len(), 1);
        assert_eq!(
            table.feature_lookups(3).unwrap().err(),
            Some(ReadError::OutOfBounds)
        );
        assert!(table.feature_lookups(2).is_none());
        assert!(table.feature_lookups(4).is_none());
    }
}
