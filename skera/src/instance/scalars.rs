//! Float region scalars and accumulation matching HarfBuzz's variation stores.
use write_fonts::{
    read::tables::variations::{DeltaSetIndex, ItemVariationStore, VariationRegion},
    types::F2Dot14,
};

pub(crate) fn region_scalar(region: &VariationRegion, coords: &[F2Dot14]) -> f32 {
    let mut gain = 1f32;
    for (i, axis) in region.region_axes().iter().enumerate() {
        let (start, peak, end) = (
            axis.start_coord().to_bits() as i32,
            axis.peak_coord().to_bits() as i32,
            axis.end_coord().to_bits() as i32,
        );
        let coord = coords.get(i).copied().unwrap_or_default().to_bits() as i32;
        if peak == 0 || coord == peak || start > peak || peak > end || start < 0 && end > 0 {
            continue;
        }
        if coord <= start || coord >= end {
            return 0.;
        }
        gain *= if coord < peak {
            (coord - start) as f32 / (peak - start) as f32
        } else {
            (end - coord) as f32 / (end - peak) as f32
        };
    }
    gain
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
