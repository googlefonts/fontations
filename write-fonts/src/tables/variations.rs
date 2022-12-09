//! OpenType variations common table formats

include!("../../generated/generated_variations.rs");

impl VariationRegionList {
    fn compute_axis_count(&self) -> usize {
        let count = self
            .variation_regions
            .first()
            .map(|reg| reg.region_axes.len())
            .unwrap_or(0);
        //TODO: check this at validation time
        debug_assert!(self
            .variation_regions
            .iter()
            .map(|reg| reg.region_axes.len())
            .all(|n| n == count));
        count
    }
}
