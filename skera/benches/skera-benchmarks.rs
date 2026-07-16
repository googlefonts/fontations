use criterion::{criterion_group, criterion_main, Criterion};
use skera::{subset_font, Plan, SubsetFlags, DEFAULT_DROP_TABLES, DEFAULT_LAYOUT_FEATURES};
use std::path::Path;
use write_fonts::{
    read::{collections::IntSet, FontRef},
    types::NameId,
};

fn read_test_font(file_name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-data/fonts")
        .join(file_name);
    std::fs::read(&path).expect("failed to read test font file")
}

fn set_from_ranges<I>(ranges: I) -> IntSet<u32>
where
    I: IntoIterator<Item = std::ops::RangeInclusive<u32>>,
{
    let mut set = IntSet::empty();
    for range in ranges {
        set.insert_range(range);
    }
    set
}

/// Creates a subsetting plan using the same default options as running the src/main.rs with
/// --unicodes.
fn create_plan(font: &FontRef, unicodes: &IntSet<u32>) -> Plan {
    Plan::new(
        &IntSet::empty(), // gids (empty by default when using --unicodes)
        unicodes,
        font,
        SubsetFlags::default(),
        &DEFAULT_DROP_TABLES.iter().copied().collect(),
        &IntSet::all(), // layout_scripts (default in CLI is all scripts)
        &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
        // Keep name IDs 0 to 6 (Copyright through PostScript Name)
        &(0..=6).map(NameId::from).collect(),
        // Keep English (US) locale (0x0409) names
        &[0x0409].into_iter().collect(),
    )
}

fn benchmark_subset(c: &mut Criterion) {
    let latin_codepoints = set_from_ranges([0x20..=0x7E, 0xA0..=0xFF, 0x100..=0x24F]);
    c.benchmark_group("subset")
        .bench_function("roboto-latin", |b| {
            let font_bytes = read_test_font("Roboto-Regular.ttf");
            b.iter(|| {
                let font = FontRef::new(&font_bytes).unwrap();
                let plan = create_plan(&font, &latin_codepoints);
                subset_font(&font, &plan).unwrap()
            });
        })
        .bench_function("roboto-variable-latin", |b| {
            let font_bytes = read_test_font("Roboto-Variable.ttf");
            b.iter(|| {
                let font = FontRef::new(&font_bytes).unwrap();
                let plan = create_plan(&font, &latin_codepoints);
                subset_font(&font, &plan).unwrap()
            });
        });
}

criterion_group!(benches, benchmark_subset);
criterion_main!(benches);
