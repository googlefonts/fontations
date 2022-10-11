use super::*;
use crate::assert_hex_eq;
use read_fonts::test_data::gpos as test_data;

#[test]
fn singleposformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-2-singleposformat1-subtable

    let table = SinglePosFormat1::read(test_data::SINGLEPOSFORMAT1).unwrap();
    let dumped = crate::write::dump_table(&table).unwrap();
    assert_hex_eq!(test_data::SINGLEPOSFORMAT1.as_ref(), &dumped);
}

#[test]
fn singleposformat2() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-3-singleposformat2-subtable

    let table = SinglePosFormat2::read(test_data::SINGLEPOSFORMAT2).unwrap();
    let dumped = crate::write::dump_table(&table).unwrap();
    assert_hex_eq!(test_data::SINGLEPOSFORMAT2.as_ref(), &dumped);
}

#[test]
fn pairposformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-4-pairposformat1-subtable

    let table = PairPosFormat1::read(test_data::PAIRPOSFORMAT1).unwrap();
    let _dumped = crate::write::dump_table(&table).unwrap();

    // we order the coverage table before the pairsets
    //assert_hex_eq!(test_data::PAIRPOSFORMAT1.as_ref(), &dumped);
}

#[test]
fn pairposformat2() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-5-pairposformat2-subtable

    let table = PairPosFormat2::read(test_data::PAIRPOSFORMAT2).unwrap();
    let dumped = crate::write::dump_table(&table).unwrap();

    // we order the coverage table before the pairsets
    assert_hex_eq!(test_data::PAIRPOSFORMAT2.as_ref(), &dumped);
}

#[test]
fn cursiveposformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-6-cursiveposformat1-subtable
    let table = CursivePosFormat1::read(test_data::CURSIVEPOSFORMAT1).unwrap();
    let _dumped = crate::write::dump_table(&table).unwrap();

    // hex is not equal because we deduplicate a table
    //assert_hex_eq!(&bytes, &dumped);
    // we order the coverage table before the pairsets
}

#[test]
fn markbaseposformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-7-markbaseposformat1-subtable

    let table = MarkBasePosFormat1::read(test_data::MARKBASEPOSFORMAT1).unwrap();
    assert_eq!(table.mark_array.mark_records.len(), 2);
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::MARKBASEPOSFORMAT1.as_ref(), &dumped);
}

#[test]
fn markligposformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-8-markligposformat1-subtable

    let table = MarkLigPosFormat1::read(test_data::MARKLIGPOSFORMAT1).unwrap();
    assert_eq!(table.mark_array.mark_records.len(), 2);
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::MARKLIGPOSFORMAT1.as_ref(), &dumped);
}

#[test]
fn markmarkposformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-9-markmarkposformat1-subtable

    let table = MarkMarkPosFormat1::read(test_data::MARKMARKPOSFORMAT1).unwrap();
    assert_eq!(table.mark2_array.mark2_records.len(), 1);
    let record = &table.mark2_array.mark2_records[0];
    assert_eq!(record.mark2_anchors.len(), 1);
    let anchor = &record.mark2_anchors[0].as_ref().unwrap();
    let anchor = match anchor {
        AnchorTable::Format1(table) => table,
        _ => panic!("wrong table format"),
    };
    assert_eq!(anchor.x_coordinate, 221);
    assert_eq!(anchor.y_coordinate, 301);
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::MARKMARKPOSFORMAT1.as_ref(), &dumped);
}

#[test]
fn contextualposformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-10-contextual-positioning-format-1

    let table =
        crate::tables::layout::SequenceContextFormat1::read(test_data::CONTEXTUALPOSFORMAT1)
            .unwrap();
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::CONTEXTUALPOSFORMAT1.as_ref(), &dumped);
}

#[test]
fn contextualposformat2() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-11-contextual-positioning-format-1

    let table =
        crate::tables::layout::SequenceContextFormat2::read(test_data::CONTEXTUALPOSFORMAT2)
            .unwrap();
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::CONTEXTUALPOSFORMAT2.as_ref(), &dumped);
}

#[test]
fn contextualposformat3() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-12-contextual-positioning-format-3

    let table =
        crate::tables::layout::SequenceContextFormat3::read(test_data::CONTEXTUALPOSFORMAT3)
            .unwrap();
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::CONTEXTUALPOSFORMAT3.as_ref(), &dumped);
}

//FIXME: turn this back on when we support device records
//#[test]
//fn valueformattable() {
//// https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-14-valueformat-table-and-valuerecord

//#[rustfmt::skip]
//let bytes = [
//0x00, 0x01, 0x00, 0x0E, 0x00, 0x99, 0x00, 0x50, 0x00, 0xD2,
//0x00, 0x18, 0x00, 0x20, 0x00, 0x02, 0x00, 0x01, 0x00, 0xC8,
//0x00, 0xD1, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x0F, 0x00, 0x01,
//0x55, 0x40, 0x00, 0x0B, 0x00, 0x0F, 0x00, 0x01, 0x55, 0x40,
//];

//let table = SinglePosFormat1::read(&bytes).unwrap();
//let table = table.to_owned_table().unwrap();
//let dumped = crate::write::dump_table(&table).unwrap();

//assert_hex_eq!(&bytes, &dumped);
//}

#[test]
fn anchorformat1() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-15-anchorformat1-table

    let table = AnchorFormat1::read(test_data::ANCHORFORMAT1).unwrap();

    assert_eq!(table.x_coordinate, 189);
    assert_eq!(table.y_coordinate, -103);
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::ANCHORFORMAT1.as_ref(), &dumped);
}

#[test]
fn anchorformat2() {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-16-anchorformat2-table

    let table = AnchorFormat2::read(test_data::ANCHORFORMAT2).unwrap();
    let dumped = crate::write::dump_table(&table).unwrap();

    assert_hex_eq!(test_data::ANCHORFORMAT2.as_ref(), &dumped);
}

//FIXME: enable when we have device tables working
//#[test]
//fn anchorformat3() {
//// https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-17-anchorformat3-table

//let bytes = [
//0x00, 0x03, 0x01, 0x17, 0x05, 0x15, 0x00, 0x0A, 0x00, 0x14,
//0x00, 0x0C, 0x00, 0x11, 0x00, 0x02, 0x11, 0x11, 0x22, 0x00,
//0x00, 0x0C, 0x00, 0x11, 0x00, 0x02, 0x11, 0x11, 0x22, 0x00,
//];
//let table = AnchorFormat3::read(&bytes).unwrap();
//let table = table.to_owned_obj(&[]).unwrap();
//let dumped = crate::write::dump_table(&table).unwrap();

//assert_hex_eq!(&bytes, &dumped);
//}

// not from the spec; this is a general test that we don't write out versioned
// fields inappropriately.
#[test]
fn no_write_versioned_fields() {
    let mut gpos = Gpos::default();

    let dumped = crate::write::dump_table(&gpos).unwrap();
    assert_eq!(dumped.len(), 12);

    gpos.feature_variations.set(FeatureVariations::default());

    let dumped = crate::write::dump_table(&gpos).unwrap();
    assert_eq!(dumped.len(), 12 + 12); // 4byte offset, 4byte version, 4byte record count
}
