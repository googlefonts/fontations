//! Regression tests for the ISO OFF extended layout formats.

use super::*;
use crate::{dump_table, read::FontRead};

#[test]
fn wide_coverage_roundtrips_without_truncation() {
    let coverage: CoverageTable = [65535, 65536, 0xFFFFFF]
        .map(GlyphId::new)
        .into_iter()
        .collect();
    let bytes = dump_table(&coverage).unwrap();
    assert_eq!(&bytes[..5], &[0, 3, 0, 0, 3]);
    assert_eq!(bytes.len(), 14);
    let read = read_fonts::tables::layout::CoverageTable::read(FontData::new(&bytes)).unwrap();
    assert_eq!(read.get(GlyphId::new(65536)), Some(1));
    let owned: CoverageTable = read.to_owned_table();
    assert_eq!(owned, coverage);
}

#[test]
fn wide_coverage_count() {
    let coverage: CoverageTable = (0..=65536).map(GlyphId::new).collect();
    let bytes = dump_table(&coverage).unwrap();
    assert_eq!(&bytes[..5], &[0, 3, 1, 0, 1]);
    let read = read_fonts::tables::layout::CoverageTable::read(FontData::new(&bytes)).unwrap();
    assert_eq!(read.get(GlyphId::new(65536)), Some(65536));
    assert_eq!(read.population(), 65537);
}

#[test]
fn wide_class_def_roundtrips_both_class_widths() {
    for items in [
        [(65536, 7), (65537, 7)],
        [(65536, 65536), (65537, 0xFFFFFF)],
    ] {
        let class: ClassDef = items
            .into_iter()
            .map(|(g, c)| (GlyphId::new(g), c))
            .collect();
        let bytes = dump_table(&class).unwrap();
        assert_eq!(bytes.len(), if items[0].1 > 65535 { 14 } else { 13 });
        let read = read_fonts::tables::layout::ClassDef::read(FontData::new(&bytes)).unwrap();
        for (g, c) in items {
            assert_eq!(read.get(GlyphId::new(g)), c);
        }
        let owned: ClassDef = read.to_owned_table();
        assert_eq!(owned, class);
    }
}

#[test]
fn small_glyphs_still_use_legacy_formats() {
    let coverage: CoverageTable = [1, 2, 3].map(GlyphId::new).into_iter().collect();
    assert!(matches!(
        coverage,
        CoverageTable::Format1(_) | CoverageTable::Format2(_)
    ));
    let class: ClassDef = [(GlyphId::new(1), 2)].into_iter().collect();
    assert!(matches!(class, ClassDef::Format1(_) | ClassDef::Format2(_)));
}
