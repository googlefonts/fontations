//! Float region scalars and accumulation matching HarfBuzz's variation stores.
use read_fonts::{
    tables::variations::{DeltaSetIndex, ItemVariationStore, VariationRegion},
    types::F2Dot14,
};

pub(crate) fn region_scalar(region: &VariationRegion, coords: &[F2Dot14]) -> f32 {
    region.compute_scalar_f32(coords)
}

pub(super) struct VariationScalars(Vec<f32>);
impl VariationScalars {
    pub fn new(store: &ItemVariationStore, coords: &[F2Dot14]) -> Option<Self> {
        let list = store.variation_region_list().ok()?;
        Some(Self(
            list.variation_regions()
                .iter()
                .map(|r| Some(region_scalar(&r.ok()?, coords)))
                .collect::<Option<Vec<_>>>()?,
        ))
    }
    pub fn delta(&self, store: &ItemVariationStore, index: DeltaSetIndex) -> Option<f32> {
        if index == DeltaSetIndex::NO_VARIATION_INDEX {
            return Some(0.);
        }
        let data = store
            .item_variation_data()
            .get(index.outer as usize)?
            .ok()?;
        if index.inner >= data.item_count() {
            return None;
        }
        let mut sum = 0f32;
        for (region, delta) in data
            .region_indexes()
            .iter()
            .zip(data.delta_set(index.inner))
        {
            let scalar = self.0.get(region.get() as usize)?;
            sum += scalar * delta as f32;
        }
        Some(sum)
    }

    /// HarfBuzz uses float region scalars but double products and accumulation
    /// for condition signs, so large cancelling deltas retain their low bits.
    pub fn condition_delta(&self, store: &ItemVariationStore, index: DeltaSetIndex) -> Option<f64> {
        if index == DeltaSetIndex::NO_VARIATION_INDEX {
            return Some(0.);
        }
        let data = store
            .item_variation_data()
            .get(index.outer as usize)?
            .ok()?;
        if index.inner >= data.item_count() {
            return None;
        }
        let mut sum = 0f64;
        for (region, delta) in data
            .region_indexes()
            .iter()
            .zip(data.delta_set(index.inner))
        {
            let scalar = self.0.get(region.get() as usize)?;
            sum += *scalar as f64 * delta as f64;
        }
        Some(sum)
    }
}
