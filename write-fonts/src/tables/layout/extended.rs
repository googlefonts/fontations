//! Regression tests for the ISO OFF extended layout formats.

use super::*;
use crate::dump_table;
use read_fonts::FontRead;

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

#[test]
fn wide_context_rules_keep_narrow_counts_and_indices() {
    let gid = GlyphId24::new(65536);
    let rec = SequenceLookupRecord::new(7, 0xFFFF);
    let rule = SequenceRule2::new(vec![gid], vec![rec]);
    let bytes = dump_table(&rule).unwrap();
    assert_eq!(bytes, [0, 2, 0, 1, 1, 0, 0, 0, 7, 0xFF, 0xFF]);
    let set = SequenceRuleSet2::new(vec![rule]);
    let bytes = dump_table(&set).unwrap();
    // The ISO SequenceRuleSet2 uses Offset16, despite the widened rule.
    assert_eq!(&bytes[..4], &[0, 1, 0, 4]);
    let read = read_fonts::tables::layout::SequenceRuleSet2::read(FontData::new(&bytes)).unwrap();
    assert_eq!(
        read.seq_rules().get(0).unwrap().input_sequence()[0].get(),
        gid
    );
    let owned: SequenceRuleSet2 = read.to_owned_table();
    assert_eq!(owned, set);
}

#[test]
fn all_wide_context_formats_roundtrip() {
    let gid = GlyphId24::new(65536);
    let coverage = CoverageTable::Format3(CoverageFormat3::new(vec![gid]));
    let class = ClassDef::Format4(ClassDefFormat4::new(vec![ClassRangeRecord2::new(
        gid, gid, 1,
    )]));
    let lookup = SequenceLookupRecord::new(0, 0xFFFF);
    let rule = SequenceRule2::new(vec![gid], vec![lookup.clone()]);
    let class_rule = ClassSequenceRule::new(vec![1], vec![lookup.clone()]);
    let chained_rule =
        ChainedSequenceRule2::new(vec![gid], vec![gid], vec![gid], vec![lookup.clone()]);
    let chained_class_rule =
        ChainedClassSequenceRule::new(vec![1], vec![1], vec![1], vec![lookup.clone()]);
    let contexts = [
        SequenceContext::Format4(SequenceContextFormat4::new(
            coverage.clone(),
            vec![Some(SequenceRuleSet2::new(vec![rule]))],
        )),
        SequenceContext::Format5(SequenceContextFormat5::new(
            coverage.clone(),
            class.clone(),
            vec![Some(ClassSequenceRuleSet2::new(vec![class_rule]))],
        )),
        SequenceContext::Format6(SequenceContextFormat6::new(
            vec![coverage.clone()],
            vec![lookup],
        )),
    ];
    for context in contexts {
        let bytes = dump_table(&context).unwrap();
        let read =
            read_fonts::tables::layout::SequenceContext::read(FontData::new(&bytes)).unwrap();
        let owned: SequenceContext = read.to_owned_table();
        assert_eq!(owned, context);
    }
    let contexts = [
        ChainedSequenceContext::Format4(ChainedSequenceContextFormat4::new(
            coverage.clone(),
            vec![Some(ChainedSequenceRuleSet2::new(vec![chained_rule]))],
        )),
        ChainedSequenceContext::Format5(ChainedSequenceContextFormat5::new(
            coverage,
            class.clone(),
            class.clone(),
            class,
            vec![Some(ChainedClassSequenceRuleSet2::new(vec![
                chained_class_rule,
            ]))],
        )),
    ];
    for context in contexts {
        let bytes = dump_table(&context).unwrap();
        let read = read_fonts::tables::layout::ChainedSequenceContext::read(FontData::new(&bytes))
            .unwrap();
        let owned: ChainedSequenceContext = read.to_owned_table();
        assert_eq!(owned, context);
    }
}

#[test]
fn wide_context_coverage_offsets_are_24bit() {
    let context = SequenceContextFormat6::new(
        vec![CoverageTable::Format3(CoverageFormat3::new(vec![
            GlyphId24::new(65536),
        ]))],
        vec![SequenceLookupRecord::new(0, 0xFFFF)],
    );
    let bytes = dump_table(&context).unwrap();
    assert_eq!(&bytes[..6], &[0, 6, 0, 1, 0, 1]);
    assert_eq!(&bytes[6..9], &[0, 0, 13]);
    assert_eq!(&bytes[9..13], &[0, 0, 0xFF, 0xFF]);
}
