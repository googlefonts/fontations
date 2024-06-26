//! Common helpers

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use skrifa::{
    instance::{Location, Size},
    FontRef, MetadataProvider,
};

/// Creates a rng seeded by data so reproduction from a fuzzer report will yield same values
pub(crate) fn create_rng(data: &[u8]) -> ChaCha8Rng {
    let mut seed = [0u8; 32];
    for (i, entry) in seed.iter_mut().enumerate() {
        *entry = data.get(i).copied().unwrap_or_default();
    }
    ChaCha8Rng::from_seed(seed)
}

pub(crate) fn create_axis_location(rng: &mut impl Rng) -> Vec<f32> {
    (0..8).map(|_| rng.gen_range(-1000.0..1000.0)).collect()
}

pub(crate) fn fuzz_sizes() -> Vec<Size> {
    vec![Size::unscaled(), Size::new(64.0), Size::new(512.0)]
}

pub(crate) fn create_location(font: &FontRef, axis_positions: &[f32]) -> Location {
    let raw_location = font
        .axes()
        .iter()
        .zip(axis_positions)
        .map(|(axis, pos)| (axis.tag(), *pos))
        .collect::<Vec<_>>();
    font.axes().location(raw_location)
}
