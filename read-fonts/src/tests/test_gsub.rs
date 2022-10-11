use super::*;
use crate::test_data::gsub as test_data;

#[test]
fn singlesubstformat1() {
    // https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-2-singlesubstformat1-subtable
    let table = SingleSubstFormat1::read(test_data::SINGLESUBSTFORMAT1_TABLE).unwrap();
    assert_eq!(table.delta_glyph_id(), 192);
}

#[test]
fn singlesubstformat2() {
    // https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-3-singlesubstformat2-subtable
    let table = SingleSubstFormat2::read(test_data::SINGLESUBSTFORMAT2_TABLE).unwrap();
    assert_eq!(
        table.substitute_glyph_ids(),
        &[
            GlyphId::new(305),
            GlyphId::new(309),
            GlyphId::new(318),
            GlyphId::new(323)
        ],
    );
}

#[test]
fn multiplesubstformat1() {
    // https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-4-multiplesubstformat1-subtable
    let table = MultipleSubstFormat1::read(test_data::MULTIPLESUBSTFORMAT1_TABLE).unwrap();
    assert_eq!(table.sequences().count(), 1);
    let seq0 = table.sequences().next().unwrap().unwrap();
    assert_eq!(
        seq0.substitute_glyph_ids(),
        &[GlyphId::new(26), GlyphId::new(26), GlyphId::new(29)]
    );
}

#[test]
fn alternatesubstformat1() {
    // https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-5-alternatesubstformat-1-subtable
    let table = AlternateSubstFormat1::read(test_data::ALTERNATESUBSTFORMAT1_TABLE).unwrap();
    assert_eq!(table.alternate_sets().count(), 1);
    let altset0 = table.alternate_sets().next().unwrap().unwrap();
    assert_eq!(
        altset0.alternate_glyph_ids(),
        &[GlyphId::new(0xc9), GlyphId::new(0xca)]
    );
}

#[test]
fn ligaturesubstformat1() {
    // https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-6-ligaturesubstformat1-subtable
    let table = LigatureSubstFormat1::read(test_data::LIGATURESUBSTFORMAT1_TABLE).unwrap();
    assert_eq!(table.ligature_sets().count(), 2);
    let ligset0 = table.ligature_sets().next().unwrap().unwrap();

    assert_eq!(ligset0.ligatures().count(), 1);
    let lig0 = ligset0.ligatures().next().unwrap().unwrap();
    assert_eq!(lig0.ligature_glyph(), GlyphId::new(347));
    assert_eq!(
        lig0.component_glyph_ids(),
        &[GlyphId::new(0x28), GlyphId::new(0x17)]
    );

    let ligset1 = table.ligature_sets().nth(1).unwrap().unwrap();
    let lig0 = ligset1.ligatures().next().unwrap().unwrap();
    assert_eq!(lig0.ligature_glyph(), GlyphId::new(0xf1));
    assert_eq!(
        lig0.component_glyph_ids(),
        &[GlyphId::new(0x1a), GlyphId::new(0x1d)]
    );
}

//TODO:
// - https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-7-contextual-substitution-format-1
// - https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-8-contextual-substitution-format-2
// - https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-9-contextual-substitution-format-3
// - https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#example-10-reversechainsinglesubstformat1-subtable
