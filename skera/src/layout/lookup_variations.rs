//! Retention and remapping of lookup-oriented feature variations.

use super::*;

pub(super) fn features_with_retained_lookups(
    variations: &FeatureVariations<'_>,
    retained: &IntSet<u16>,
) -> IntSet<u16> {
    let mut features = IntSet::empty();
    for record in variations.lookup_variation_records().unwrap_or_default() {
        if record.feature_lookups_offset().is_null() {
            continue;
        }
        let Ok(table) = record.feature_lookups(variations.offset_data()) else {
            return IntSet::empty();
        };
        if u64::from(table.lookup_condition_count()) * 8
            > table.offset_data().len().saturating_sub(10) as u64
        {
            return IntSet::empty();
        }
        for condition in table.lookup_condition_records() {
            if condition.lookup_index_list_offset().is_null() {
                continue;
            }
            let Ok(indices) = condition.lookup_index_list(table.offset_data()) else {
                return IntSet::empty();
            };
            if indices.min_table_bytes().is_empty() {
                return IntSet::empty();
            }
            if indices
                .lookup_indices()
                .iter()
                .any(|i| retained.contains(i.get()))
            {
                features.insert(record.feature_index());
                break;
            }
        }
    }
    features
}

pub(crate) fn protect_lookup_variation_features(
    variations: &FeatureVariations<'_>,
    duplicates: &mut FnvHashMap<u16, u16>,
) {
    let protected: IntSet<u16> = variations
        .lookup_variation_records()
        .unwrap_or_default()
        .iter()
        .map(|record| record.feature_index())
        .collect();
    // Lookup variations are feature-indexed: matching default lookups alone
    // do not make these features, or their default-only duplicates, interchangeable.
    for (feature, duplicate) in duplicates {
        if protected.contains(*feature) || protected.contains(*duplicate) {
            *duplicate = *feature;
        }
    }
}

#[cfg(test)]
mod tests;
