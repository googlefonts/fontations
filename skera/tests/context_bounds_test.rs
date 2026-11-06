//! Classes beyond the rule-set array do not invalidate class-context lookups.
use skera::{subset_font, Plan};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{FontData, FontRead, FontRef, TableProvider},
    tables::{gpos::*, gsub::*, layout::*},
    types::{GlyphId16, Tag},
    FontBuilder,
};

fn source(chained: bool, empty: bool) -> Vec<u8> {
    let font = FontRef::new(include_bytes!(
        "../test-data/fonts/layout-feature-dedup.ttf"
    ))
    .unwrap();
    let coverage = || CoverageTable::from_iter([GlyphId16::new(1), GlyphId16::new(2)]);
    let classes = || ClassDef::format_1(GlyphId16::new(1), vec![1, 2]);
    let scripts = || {
        ScriptList::new(vec![ScriptRecord::new(
            Tag::new(b"DFLT"),
            Script::new(Some(LangSys::new(vec![0])), vec![]),
        )])
    };
    let features =
        |tag| FeatureList::new(vec![FeatureRecord::new(tag, Feature::new(None, vec![0]))]);
    // Coverage contains class 2, but only classes 0 and 1 have rule-set slots.
    // The omitted trailing class has no matching rules.
    let context = SequenceContext::Format2(SequenceContextFormat2::new(
        coverage(),
        classes(),
        if empty {
            vec![]
        } else {
            vec![
                None,
                Some(ClassSequenceRuleSet::new(vec![ClassSequenceRule::new(
                    vec![1],
                    vec![SequenceLookupRecord::new(0, 1)],
                )])),
            ]
        },
    ));
    let chain = ChainedSequenceContext::Format2(ChainedSequenceContextFormat2::new(
        coverage(),
        ClassDef::format_2(vec![]),
        classes(),
        ClassDef::format_2(vec![]),
        if empty {
            vec![]
        } else {
            vec![
                None,
                Some(ChainedClassSequenceRuleSet::new(vec![
                    ChainedClassSequenceRule::new(
                        vec![],
                        vec![1],
                        vec![],
                        vec![SequenceLookupRecord::new(0, 1)],
                    ),
                ])),
            ]
        },
    ));
    let gsub = Gsub::new(
        scripts(),
        features(Tag::new(b"calt")),
        SubstitutionLookupList::new(vec![
            if chained {
                SubstitutionLookup::ChainContextual(Lookup::new(
                    LookupFlag::empty(),
                    vec![chain.clone().into()],
                ))
            } else {
                SubstitutionLookup::Contextual(Lookup::new(
                    LookupFlag::empty(),
                    vec![context.clone().into()],
                ))
            },
            SubstitutionLookup::Single(Lookup::new(
                LookupFlag::empty(),
                vec![SingleSubst::format_2(
                    CoverageTable::from_iter([GlyphId16::new(1)]),
                    vec![GlyphId16::new(3)],
                )],
            )),
        ]),
    );
    let gpos = Gpos::new(
        scripts(),
        features(Tag::new(b"kern")),
        PositionLookupList::new(vec![
            if chained {
                PositionLookup::ChainContextual(Lookup::new(
                    LookupFlag::empty(),
                    vec![chain.into()],
                ))
            } else {
                PositionLookup::Contextual(Lookup::new(LookupFlag::empty(), vec![context.into()]))
            },
            PositionLookup::Single(Lookup::new(
                LookupFlag::empty(),
                vec![SinglePos::format_1(
                    CoverageTable::from_iter([GlyphId16::new(1)]),
                    ValueRecord::new().with_x_advance(-25),
                )],
            )),
        ]),
    );
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
    }
    builder.add_table(&gsub).unwrap();
    builder.add_table(&gpos).unwrap();
    builder.build()
}

#[test]
fn trailing_classes_without_rule_sets_preserve_context_and_chained_context() {
    for chained in [false, true] {
        let bytes = source(chained, false);
        let font = FontRef::new(&bytes).unwrap();
        let output = subset_font(&font, &Plan::keep_everything(&font)).unwrap();
        let font = FontRef::new(&output).unwrap();
        assert_eq!(
            font.gsub().unwrap().lookup_list().unwrap().lookup_count(),
            2
        );
        assert_eq!(
            font.gpos().unwrap().lookup_list().unwrap().lookup_count(),
            2
        );
        let (gsub, gpos): (&[u8], &[u8]) = if chained {
            (
                include_bytes!("../test-data/expected/context-bounds/true.GSUB"),
                include_bytes!("../test-data/expected/context-bounds/true.GPOS"),
            )
        } else {
            (
                include_bytes!("../test-data/expected/context-bounds/false.GSUB"),
                include_bytes!("../test-data/expected/context-bounds/false.GPOS"),
            )
        };
        let expected: Gsub = write_fonts::read::tables::gsub::Gsub::read(FontData::new(gsub))
            .unwrap()
            .to_owned_table();
        let actual: Gsub = font.gsub().unwrap().to_owned_table();
        assert_eq!(actual, expected);
        let expected: Gpos = write_fonts::read::tables::gpos::Gpos::read(FontData::new(gpos))
            .unwrap()
            .to_owned_table();
        let actual: Gpos = font.gpos().unwrap().to_owned_table();
        assert_eq!(actual, expected);
    }
}

#[test]
fn zero_rule_set_counts_do_not_read_an_offset() {
    for chained in [false, true] {
        let bytes = source(chained, true);
        let font = FontRef::new(&bytes).unwrap();
        subset_font(&font, &Plan::keep_everything(&font)).unwrap();
    }
}

#[test]
fn all_glyphs_preserve_amiri_quran_positioning_like_harfbuzz() {
    let font = FontRef::new(include_bytes!("../test-data/fonts/AmiriQuran.ttf")).unwrap();
    let output = subset_font(&font, &Plan::keep_everything(&font)).unwrap();
    let font = FontRef::new(&output).unwrap();
    let reference = write_fonts::read::tables::gpos::Gpos::read(FontData::new(include_bytes!(
        "../test-data/expected/context-bounds/AmiriQuran.GPOS"
    )))
    .unwrap();
    let actual: Gpos = font.gpos().unwrap().to_owned_table();
    let expected: Gpos = reference.to_owned_table();
    assert_eq!(actual, expected);
}
