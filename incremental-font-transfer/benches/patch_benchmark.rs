use criterion::{black_box, criterion_group, criterion_main, Criterion};
use incremental_font_transfer::{
    patch_group::{PatchGroup, UrlStatus},
    patchmap::{intersecting_patches, PatchUrl, SubsetDefinition},
};
use read_fonts::{collections::IntSet, FontRef};
use std::collections::HashMap;

fn load_roboto_patches() -> HashMap<PatchUrl, UrlStatus> {
    font_test_data::ift::ROBOTO_PATCHES
        .iter()
        .map(|(name, bytes)| {
            (
                PatchUrl(name.to_string()),
                UrlStatus::Pending(bytes.to_vec()),
            )
        })
        .collect()
}

pub fn intersecting_patches_benchmark(c: &mut Criterion) {
    let font_bytes = font_test_data::ift::ROBOTO_IFT;
    let font = FontRef::new(&font_bytes).unwrap();

    c.bench_function("intersecting_patches_all", |b| {
        let subset_all = SubsetDefinition::all();
        b.iter(|| intersecting_patches(black_box(&font), black_box(&subset_all)).unwrap())
    })
    .bench_function("intersecting_patches_small", |b| {
        let subset_small = SubsetDefinition::codepoints({
            let mut cp = IntSet::empty();
            cp.insert_range(b'a' as u32..=b'c' as u32);
            cp
        });
        b.iter(|| intersecting_patches(black_box(&font), black_box(&subset_small)).unwrap())
    });
}

pub fn select_next_patches_benchmark(c: &mut Criterion) {
    let font = FontRef::new(&font_test_data::ift::ROBOTO_IFT).unwrap();
    let patch_data = load_roboto_patches();

    c.bench_function("select_next_patches_all", |b| {
        let subset_all = SubsetDefinition::all();
        b.iter(|| {
            PatchGroup::select_next_patches(
                black_box(font.clone()),
                black_box(&patch_data),
                black_box(&subset_all),
            )
            .unwrap()
        })
    });
}

criterion_group!(
    benches,
    intersecting_patches_benchmark,
    select_next_patches_benchmark,
);
criterion_main!(benches);
