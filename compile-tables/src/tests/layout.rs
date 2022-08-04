use super::*;
use crate::assert_hex_eq;
use font_tables::test_data::layout as test_data;

#[test]
fn example_1_scripts() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-1-scriptlist-table-and-scriptrecords

    let table = ScriptList::read(test_data::SCRIPTS).unwrap();
    let _dumped = crate::dump_table(&table);
    //NOTE: we can't roundtrip this because the data doesn't include subtables.
    //assert_hex_eq!(&bytes, &dumped);
}

#[test]
fn example_2_scripts_and_langs() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-2-script-table-langsysrecord-and-langsys-table

    let table = Script::read(test_data::SCRIPTS_AND_LANGUAGES).unwrap();
    let dumped = crate::dump_table(&table);
    assert_hex_eq!(test_data::SCRIPTS_AND_LANGUAGES.as_ref(), &dumped);
}

#[test]
fn example_3_featurelist_and_feature() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-3-featurelist-table-and-feature-table
    let table = FeatureList::read(test_data::FEATURELIST_AND_FEATURE).unwrap();
    let dumped = crate::dump_table(&table);
    assert_hex_eq!(test_data::FEATURELIST_AND_FEATURE.as_ref(), &dumped);
}
