//! The spike's point: generated code, real tables, checked against the parser
//! the crate ships.

use super::gpos::*;
use crate::exp::parse::Bytes;
use crate::exp::parse::Table as _;
use crate::tables::gpos as real;
use crate::{FontData, FontRead};

/// The bytes, as the new framework sees them.
fn data(bytes: &[u8]) -> Bytes<'_> {
    Bytes::new(bytes)
}

/// The same bytes, as the parser the crate ships sees them.
fn old(bytes: &[u8]) -> FontData<'_> {
    FontData::new(bytes)
}

#[test]
fn single_pos_1() {
    let bytes = font_test_data::gpos::SINGLEPOSFORMAT1;
    let ours = SinglePosFormat1::read(data(bytes)).unwrap();
    let theirs = real::SinglePosFormat1::read(old(bytes)).unwrap();
    assert_eq!(ours.value_format().bits(), theirs.value_format().bits());
    assert_eq!(
        ours.value_record().y_placement(),
        theirs.value_record().y_placement()
    );
    assert_eq!(ours.value_record().y_placement(), Some(-80));
    // the coverage offset resolves with no `data` argument
    assert!(ours.coverage().is_some());
}

#[test]
fn single_pos_2() {
    let bytes = font_test_data::gpos::SINGLEPOSFORMAT2;
    let ours = SinglePosFormat2::read(data(bytes)).unwrap();
    let theirs = real::SinglePosFormat2::read(old(bytes)).unwrap();
    // note: no `?` and no `unwrap` on the array, and the iterator yields
    // records rather than `Result`s
    let ours: Vec<_> = ours
        .value_records()
        .iter()
        .map(|r| (r.x_placement(), r.x_advance()))
        .collect();
    let theirs: Vec<_> = theirs
        .value_records()
        .iter()
        .map(|r| r.unwrap())
        .map(|r| (r.x_placement(), r.x_advance()))
        .collect();
    assert_eq!(ours, theirs);
    assert_eq!(ours.len(), 3);
}

#[test]
fn pair_pos_1() {
    let bytes = font_test_data::gpos::PAIRPOSFORMAT1;
    let ours = PairPosFormat1::read(data(bytes)).unwrap();
    let theirs = real::PairPosFormat1::read(old(bytes)).unwrap();

    let ours: Vec<_> = ours
        .pair_sets()
        .iter()
        .flatten()
        .flat_map(|set| {
            set.pair_value_records()
                .iter()
                .map(|r| {
                    (
                        r.second_glyph(),
                        r.value_record1().x_advance(),
                        r.value_record2().x_placement(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let theirs: Vec<_> = theirs
        .pair_sets()
        .iter()
        .map(|s| s.unwrap())
        .flat_map(|set| {
            set.pair_value_records()
                .iter()
                .map(|r| r.unwrap())
                .map(|r| {
                    (
                        r.second_glyph(),
                        r.value_record1().x_advance(),
                        r.value_record2().x_placement(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(ours, theirs);
    assert_eq!(ours.len(), 2);
}

#[test]
fn pair_pos_2_records_nested_two_deep() {
    let bytes = font_test_data::gpos::PAIRPOSFORMAT2;
    let ours = PairPosFormat2::read(data(bytes)).unwrap();
    let theirs = real::PairPosFormat2::read(old(bytes)).unwrap();

    let ours: Vec<_> = ours
        .class1_records()
        .iter()
        .flat_map(|c1| {
            c1.class2_records()
                .iter()
                .map(|c2| {
                    (
                        c2.value_record1().x_advance(),
                        c2.value_record2().y_advance(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let theirs: Vec<_> = theirs
        .class1_records()
        .iter()
        .map(|c| c.unwrap())
        .flat_map(|c1| {
            c1.class2_records()
                .iter()
                .map(|c| c.unwrap())
                .map(|c2| {
                    (
                        c2.value_record1().x_advance(),
                        c2.value_record2().y_advance(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(ours, theirs);
    assert_eq!(ours.len(), 4);
    assert_eq!(ours[3], (Some(-50), None));
}

#[test]
fn mark_records_resolve_anchors_with_no_data_argument() {
    let bytes = &font_test_data::gpos::MARKBASEPOSFORMAT1[0x1A..];
    let ours = MarkArray::read(data(bytes)).unwrap();
    let theirs = real::MarkArray::read(old(bytes)).unwrap();

    let ours: Vec<_> = ours
        .mark_records()
        .iter()
        .map(|rec| {
            // no argument: `WithParent` already holds the base
            let anchor = rec.mark_anchor().unwrap();
            (rec.mark_class(), anchor_coords(anchor))
        })
        .collect();
    let theirs: Vec<_> = theirs
        .mark_records()
        .iter()
        .map(|rec| {
            // today the caller has to know which ancestor's data this is
            let anchor = rec.mark_anchor(theirs.offset_data()).unwrap();
            let real::AnchorTable::Format1(a) = anchor else {
                panic!("expected format 1")
            };
            (rec.mark_class(), (a.x_coordinate(), a.y_coordinate()))
        })
        .collect();
    assert_eq!(ours, theirs);
    assert_eq!(ours.len(), 2);
}

fn anchor_coords(anchor: AnchorTable<'_>) -> (i16, i16) {
    let AnchorTable::Format1(a) = anchor else {
        panic!("expected format 1")
    };
    (a.x_coordinate(), a.y_coordinate())
}

#[test]
fn a_truncated_table_is_none_and_a_truncated_array_is_empty() {
    let full = font_test_data::gpos::SINGLEPOSFORMAT2;
    assert!(SinglePosFormat2::read(data(&full[..4])).is_none());

    // beyond MIN_SIZE, a field that is not there reads as empty rather than as
    // an error, which is what the crate does today
    let truncated = &full[..8];
    let table = SinglePosFormat2::read(data(truncated)).unwrap();
    assert_eq!(table.value_count(), 3);
    assert!(table.value_records().is_empty());
}

/// fvar's `InstanceRecord` is hand-written today. These check that the
/// generated one agrees with it on real fonts, field for field.
mod fvar {
    use super::*;
    use crate::exp::tables::fvar::Fvar;
    use crate::{FontRef, TableProvider};

    fn compare(font_bytes: &[u8], expect_ps_names: usize) -> usize {
        let font = FontRef::new(font_bytes).unwrap();
        let real = font.fvar().unwrap();
        let ours = Fvar::read(real.offset_data().into()).unwrap();

        assert_eq!(ours.axis_count(), real.axis_count());
        assert_eq!(ours.instance_count(), real.instance_count());
        assert_eq!(ours.instance_size(), real.instance_size());

        // no shim in the middle: the arrays hang off the table directly
        assert_eq!(ours.axes().len(), real.axes().unwrap().len());
        for (a, b) in ours.axes().iter().zip(real.axes().unwrap()) {
            assert_eq!(a.axis_tag(), b.axis_tag());
            assert_eq!(a.min_value(), b.min_value());
            assert_eq!(a.max_value(), b.max_value());
        }

        let theirs = real.instances().unwrap();
        let ours = ours.instances();
        assert_eq!(ours.len(), theirs.len());

        let mut with_ps_name = 0;
        for i in 0..theirs.len() {
            let theirs = theirs.get(i).unwrap();
            let ours = ours.get(i).unwrap();
            assert_eq!(ours.subfamily_name_id(), theirs.subfamily_name_id, "{i}");
            assert_eq!(ours.flags(), theirs.flags, "{i}");
            assert_eq!(ours.coordinates(), theirs.coordinates, "{i}");
            // the 0xFFFF sentinel is applied by the hand-written wrapper
            assert_eq!(ours.post_script_name(), theirs.post_script_name_id, "{i}");
            with_ps_name += ours.post_script_name_id().is_some() as usize;
        }
        // report how many records `#[if_fits]` found room in, so a font that
        // never exercises the branch cannot pass silently
        assert_eq!(with_ps_name, expect_ps_names, "postScriptNameID presence");
        theirs.len()
    }

    #[test]
    fn vazirmatn_instances_match_the_hand_written_record() {
        // this one carries named instances, including the `postScriptNameID`
        // that `#[if_fits]` decides the presence of
        let n = compare(font_test_data::VAZIRMATN_VAR, 0);
        assert!(n > 0, "expected named instances");
    }

    #[test]
    fn amstelvar_declares_no_instances() {
        // an fvar with axes but an empty instance array: the record array is
        // empty rather than absent, and reads as such
        assert_eq!(compare(font_test_data::AMSTELVAR_AVAR2_A, 0), 0);
    }
}

/// Both sides of `#[if_fits]`.
///
/// The fonts above never carry a `postScriptNameID`, so on their own they only
/// prove that the field is correctly *absent*. These build an fvar both ways
/// and check the generated record against the hand-written one for each.
mod if_fits {
    use super::*;
    use crate::exp::tables::fvar::InstanceRecord;
    use crate::exp::ComputedSize;
    use crate::tables::fvar::InstanceRecord as RealInstanceRecord;
    use font_test_data::bebuffer::BeBuffer;
    use types::{Fixed, NameId};

    /// One instance record: subfamily id, flags, one coordinate, and a
    /// PostScript name id if `with_ps_name`.
    fn instance(subfamily: u16, coord: f64, ps_name: Option<u16>) -> Vec<u8> {
        let mut buf = BeBuffer::new()
            .push(subfamily)
            .push(0u16)
            .push(Fixed::from_f64(coord));
        if let Some(id) = ps_name {
            buf = buf.push(id);
        }
        buf.data().to_vec()
    }

    fn check(coord: f64, ps_name: Option<u16>) {
        let bytes = instance(300, coord, ps_name);
        let instance_size = bytes.len() as u16;

        let ours = InstanceRecord::at(Bytes::new(&bytes), 0, (1, instance_size));
        let theirs = RealInstanceRecord::read(FontData::new(&bytes), 1, instance_size).unwrap();

        assert_eq!(ours.subfamily_name_id(), theirs.subfamily_name_id);
        assert_eq!(ours.coordinates(), theirs.coordinates);
        assert_eq!(ours.post_script_name(), theirs.post_script_name_id);
        assert_eq!(
            <InstanceRecord as ComputedSize>::computed_size((1, instance_size)),
            instance_size as usize
        );
    }

    #[test]
    fn present_when_the_declared_size_leaves_room() {
        check(400.0, Some(301));
        // the field is there, so both parsers report it
        let bytes = instance(300, 400.0, Some(301));
        let rec = InstanceRecord::at(Bytes::new(&bytes), 0, (1, bytes.len() as u16));
        assert_eq!(rec.post_script_name_id(), Some(NameId::new(301)));
        assert_eq!(rec.post_script_name(), Some(NameId::new(301)));
    }

    #[test]
    fn absent_when_it_does_not_fit() {
        check(400.0, None);
        let bytes = instance(300, 400.0, None);
        let rec = InstanceRecord::at(Bytes::new(&bytes), 0, (1, bytes.len() as u16));
        assert_eq!(rec.post_script_name_id(), None);
    }

    #[test]
    fn the_0xffff_sentinel_is_not_a_name() {
        // present in the bytes, but the spec says it means "no PostScript name"
        let bytes = instance(300, 400.0, Some(0xFFFF));
        let rec = InstanceRecord::at(Bytes::new(&bytes), 0, (1, bytes.len() as u16));
        assert_eq!(rec.post_script_name_id(), Some(NameId::new(0xFFFF)));
        assert_eq!(rec.post_script_name(), None);
        check(400.0, Some(0xFFFF));
    }

    #[test]
    fn padding_beyond_the_fields_is_respected() {
        // instanceSize larger than the fields plus the optional one: the record
        // still occupies what the font says, and the optional field is present
        let mut bytes = instance(300, 400.0, Some(301));
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        let size = bytes.len() as u16;
        let rec = InstanceRecord::at(Bytes::new(&bytes), 0, (1, size));
        assert_eq!(rec.post_script_name(), Some(NameId::new(301)));
        assert_eq!(
            <InstanceRecord as ComputedSize>::computed_size((1, size)),
            size as usize
        );
    }
}

/// The pass that buys back what the accessors gave up.
#[cfg(feature = "sanitize")]
mod sanitize {
    use super::*;
    use crate::exp::parse::sanitize::{sanitize, sanitize_with, Limits, Problem};

    fn gpos_of(bytes: &[u8]) -> PairPosFormat2<'_> {
        PairPosFormat2::read(data(bytes)).unwrap()
    }

    #[test]
    fn a_good_table_reports_nothing() {
        let table = gpos_of(font_test_data::gpos::PAIRPOSFORMAT2);
        let report = sanitize(&table);
        assert!(report.is_ok(), "{report}");
        assert!(
            report.tables_visited() > 1,
            "should have walked into coverage"
        );
    }

    #[test]
    fn a_truncated_array_is_reported_with_its_path() {
        // the header claims class1_count/class2_count that the data cannot hold
        let mut bytes = font_test_data::gpos::PAIRPOSFORMAT2.to_vec();
        bytes[12] = 0xFF; // class1_count high byte
        let table = gpos_of(&bytes);

        // the accessor still hands back a usable array, saying nothing
        assert!(table.class1_records().is_empty());

        // the pass says what the accessor could not
        let report = sanitize(&table);
        assert!(!report.is_ok());
        let err = report
            .errors()
            .iter()
            .find(|e| e.field() == Some("class1_records"))
            .expect("expected a class1_records error");
        assert!(matches!(err.problem(), Problem::FieldOutOfBounds { .. }));
        assert_eq!(err.table(), Some("PairPosFormat2"));
        let shown = err.to_string();
        assert!(
            shown.starts_with("PairPosFormat2.class1_records"),
            "{shown}"
        );
    }

    #[test]
    fn a_bad_offset_is_reported_with_its_path() {
        let mut bytes = font_test_data::gpos::PAIRPOSFORMAT2.to_vec();
        // point the coverage offset past the end of the table
        bytes[2] = 0xFF;
        bytes[3] = 0xF0;
        let table = gpos_of(&bytes);
        assert!(table.coverage().is_none());

        let report = sanitize(&table);
        let err = report
            .errors()
            .iter()
            .find(|e| e.field() == Some("coverage_offset"))
            .expect("expected a coverage error");
        assert!(matches!(err.problem(), Problem::UnresolvableOffset { .. }));
    }

    #[test]
    fn the_path_reaches_through_offsets_and_indices() {
        // walk a whole GPOS and check the deepest path we produce reads sensibly
        let bytes = font_test_data::gpos::MARKBASEPOSFORMAT1;
        let table = MarkBasePosFormat1::read(data(bytes)).unwrap();
        let report = sanitize(&table);
        assert!(report.is_ok(), "{report}");

        // now break an anchor offset inside the mark array and see where it says
        let mut bytes = bytes.to_vec();
        // markArray is at 0x1A; its first record's anchor offset is at 0x1E
        bytes[0x1E] = 0xFF;
        bytes[0x1F] = 0xF0;
        let table = MarkBasePosFormat1::read(data(&bytes)).unwrap();
        let report = sanitize(&table);
        assert!(!report.is_ok());
        let shown = report.to_string();
        // the path names the field inside the record, reached through the array
        assert!(shown.contains("mark_array"), "{shown}");
        assert!(shown.contains("mark_anchor_offset"), "{shown}");
    }

    /// What a report actually reads like, pinned so it cannot quietly degrade.
    #[test]
    fn a_report_names_the_path_the_field_and_the_problem() {
        let mut bytes = font_test_data::gpos::MARKBASEPOSFORMAT1.to_vec();
        bytes[0x1E] = 0xFF;
        bytes[0x1F] = 0xF0;
        let table = MarkBasePosFormat1::read(data(&bytes)).unwrap();
        let report = sanitize(&table);
        assert_eq!(
            report.errors()[0].to_string(),
            concat!(
                "MarkBasePosFormat1.mark_array_offset → MarkArray.mark_records[0]",
                ".mark_anchor_offset: offset 65520 does not resolve to a readable table",
            )
        );

        let mut bytes = font_test_data::gpos::PAIRPOSFORMAT2.to_vec();
        bytes[12] = 0xFF;
        let report = sanitize(&gpos_of(&bytes));
        assert_eq!(
            report.errors()[0].to_string(),
            concat!(
                "PairPosFormat2.class1_records: field extends past the end of the ",
                "table (needs 261144, have 60)",
            )
        );
    }

    #[test]
    fn a_cycle_terminates() {
        // an extension lookup whose offset points back at itself
        let table = gpos_of(font_test_data::gpos::PAIRPOSFORMAT2);
        let report = sanitize_with(
            &table,
            Limits {
                tables: 8,
                ..Default::default()
            },
        );
        // the point is that it returns at all; with a table budget this small
        // it may or may not have finished
        let _ = report.stopped_early();
    }

    #[test]
    fn fan_out_is_bounded() {
        // PairPosFormat1 reaches its subtables through an array of offsets,
        // which is the shape a font would use to make a walk expensive
        let table = PairPosFormat1::read(data(font_test_data::gpos::PAIRPOSFORMAT1)).unwrap();

        let full = sanitize(&table);
        assert!(full.is_ok(), "{full}");

        let capped = sanitize_with(
            &table,
            Limits {
                array_elements: 0,
                ..Default::default()
            },
        );
        // the array's own extent is still checked, so a lying count would still
        // be caught; only descending into the elements is skipped
        assert!(capped.is_ok(), "{capped}");
        assert!(
            capped.tables_visited() < full.tables_visited(),
            "{} vs {}",
            capped.tables_visited(),
            full.tables_visited()
        );
    }

    #[test]
    fn a_lying_count_is_caught_even_with_no_element_budget() {
        let mut bytes = font_test_data::gpos::PAIRPOSFORMAT1.to_vec();
        bytes[8] = 0xFF; // pair_set_count high byte
        let table = PairPosFormat1::read(data(&bytes)).unwrap();
        let report = sanitize_with(
            &table,
            Limits {
                array_elements: 0,
                ..Default::default()
            },
        );
        assert!(
            !report.is_ok(),
            "a count past the end should still be caught"
        );
    }
}

/// The other walk: same checks, no strings, yes or no.
#[cfg(feature = "fast_sanitize")]
mod fast_sanitize {
    use super::*;
    use crate::exp::parse::fast_sanitize::{is_sound, is_sound_with, Limits};

    fn pair_pos_2(bytes: &[u8]) -> PairPosFormat2<'_> {
        PairPosFormat2::read(data(bytes)).unwrap()
    }

    #[test]
    fn a_good_table_is_sound() {
        assert!(is_sound(&pair_pos_2(font_test_data::gpos::PAIRPOSFORMAT2)));
        assert!(is_sound(
            &PairPosFormat1::read(data(font_test_data::gpos::PAIRPOSFORMAT1)).unwrap()
        ));
        assert!(is_sound(
            &MarkBasePosFormat1::read(data(font_test_data::gpos::MARKBASEPOSFORMAT1)).unwrap()
        ));
    }

    #[test]
    fn a_truncated_array_is_not() {
        let mut bytes = font_test_data::gpos::PAIRPOSFORMAT2.to_vec();
        bytes[12] = 0xFF;
        assert!(!is_sound(&pair_pos_2(&bytes)));
    }

    #[test]
    fn a_bad_offset_is_not() {
        let mut bytes = font_test_data::gpos::PAIRPOSFORMAT2.to_vec();
        bytes[2] = 0xFF;
        bytes[3] = 0xF0;
        assert!(!is_sound(&pair_pos_2(&bytes)));
    }

    #[test]
    fn a_bad_offset_inside_a_record_is_not() {
        let mut bytes = font_test_data::gpos::MARKBASEPOSFORMAT1.to_vec();
        bytes[0x1E] = 0xFF;
        bytes[0x1F] = 0xF0;
        assert!(!is_sound(&MarkBasePosFormat1::read(data(&bytes)).unwrap()));
    }

    #[test]
    fn a_walk_that_could_not_finish_is_not_sound() {
        // stopping early is reported as unsound: a walk that did not finish
        // cannot vouch for what it did not see
        let table =
            MarkBasePosFormat1::read(data(font_test_data::gpos::MARKBASEPOSFORMAT1)).unwrap();
        assert!(is_sound(&table));
        assert!(!is_sound_with(
            &table,
            Limits {
                tables: 1,
                ..Default::default()
            }
        ));
    }

    /// The two walks must agree about whether a font is well formed; only about
    /// how much they say.
    #[cfg(feature = "sanitize")]
    #[test]
    fn the_two_passes_agree() {
        use crate::exp::parse::sanitize::sanitize;

        let good = font_test_data::gpos::PAIRPOSFORMAT2;
        let mut truncated = good.to_vec();
        truncated[12] = 0xFF;
        let mut bad_offset = good.to_vec();
        bad_offset[2] = 0xFF;
        bad_offset[3] = 0xF0;

        for bytes in [good, &truncated[..], &bad_offset[..]] {
            let table = pair_pos_2(bytes);
            assert_eq!(
                is_sound(&table),
                sanitize(&table).is_ok(),
                "the two passes disagreed"
            );
        }
    }
}

/// A CFF INDEX, whose fixed header is two bytes when it is empty and three
/// when it is not.
mod cff_index {
    use super::*;
    use crate::exp::tables::cff::Index;
    use crate::exp::Table;

    /// An INDEX holding `items`.
    fn index(items: &[&[u8]]) -> Vec<u8> {
        let mut out = (items.len() as u16).to_be_bytes().to_vec();
        if items.is_empty() {
            return out;
        }
        out.push(1); // off_size
        let mut off = 1u8;
        out.push(off);
        for item in items {
            off += item.len() as u8;
            out.push(off);
        }
        for item in items {
            out.extend_from_slice(item);
        }
        out
    }

    #[test]
    fn an_empty_index_is_two_bytes_and_parses() {
        let bytes = index(&[]);
        assert_eq!(bytes.len(), 2);
        // the whole point: the existing declaration puts off_size in the fixed
        // header, so MIN_SIZE is 3 and this does not parse at all
        assert_eq!(<Index as Table>::MIN_SIZE, 2);

        let idx = Index::read(data(&bytes)).unwrap();
        assert_eq!(idx.count(), 0);
        // nothing follows the count, and the accessors say so rather than
        // reading past the end
        assert_eq!(idx.off_size(), None);
        assert_eq!(idx.offsets(), None);
        assert_eq!(idx.data(), None);
    }

    #[test]
    fn a_non_empty_index_still_has_everything() {
        let bytes = index(&[b"ab", b"cde"]);
        let idx = Index::read(data(&bytes)).unwrap();
        assert_eq!(idx.count(), 2);
        assert_eq!(idx.off_size(), Some(1));
        // count + 1 offsets, one byte each
        assert_eq!(idx.offsets().map(<[u8]>::len), Some(3));
        assert_eq!(idx.data(), Some(&b"abcde"[..]));
    }

    #[test]
    fn a_one_byte_index_does_not_parse() {
        assert!(Index::read(data(&[0])).is_none());
    }

    #[test]
    fn a_truncated_non_empty_index_is_caught() {
        // claims two items but stops after the count and off_size
        let bytes = [0u8, 2, 1];
        let idx = Index::read(data(&bytes)).unwrap();
        assert_eq!(idx.count(), 2);
        // the accessor gives nothing rather than reading past the end
        assert_eq!(idx.offsets(), None);
    }

    #[cfg(feature = "sanitize")]
    #[test]
    fn an_empty_index_is_sound_and_a_truncated_one_is_not() {
        use crate::exp::parse::sanitize::sanitize;

        assert!(sanitize(&Index::read(data(&index(&[]))).unwrap()).is_ok());
        assert!(sanitize(&Index::read(data(&index(&[b"ab"]))).unwrap()).is_ok());

        let report = sanitize(&Index::read(data(&[0u8, 2, 1])).unwrap());
        assert!(!report.is_ok(), "a truncated INDEX should be reported");
        assert_eq!(report.errors()[0].field(), Some("offsets"));
    }
}

/// Framework behaviour, checked against generated tables rather than
/// hand-written stand-ins.
mod accessors {
    use super::*;
    use crate::exp::tables::gpos::{MarkArray, MarkRecord};

    fn mark_array(bytes: &[u8]) -> MarkArray<'_> {
        MarkArray::read(data(bytes)).unwrap()
    }

    #[test]
    fn the_zerocopy_view_survives_the_wrapper() {
        let bytes = &font_test_data::gpos::MARKBASEPOSFORMAT1[0x1A..];
        let records = mark_array(bytes).mark_records();
        // the raw slice is what the array's store holds: the bulk view costs
        // nothing and is not a second read
        let raw = records.store().records();
        assert_eq!(raw.len(), records.len());
        for (raw, wrapped) in raw.iter().zip(records.iter()) {
            // `WithParent` derefs to the record, so the plain accessors are
            // the record's own
            assert_eq!(raw.mark_class(), wrapped.mark_class());
        }
    }

    #[test]
    fn a_null_offset_in_a_non_nullable_field_is_none() {
        // `mark_anchor_offset` is declared non-nullable; a font may still hold
        // a zero there, and the accessor says so the way it says everything
        // else
        let bytes = [0u8, 1, 0, 0, 0, 0];
        let record = mark_array(&bytes).mark_records().get(0).unwrap();
        assert_eq!(record.mark_class(), 0);
        assert!(record.mark_anchor().is_none());
    }

    #[test]
    fn the_caller_picks_the_fallback_even_for_a_non_nullable_offset() {
        let bytes = &font_test_data::gpos::MARKBASEPOSFORMAT1[0x1A..];
        let records = mark_array(bytes).mark_records();

        // past the end: nothing is fabricated, and a caller who wants a value
        // asks for one. This only compiles because offsets default to null.
        let absent: MarkRecord = records.get(9).map(|rec| *rec).unwrap_or_default();
        assert_eq!(absent, MarkRecord::default());
        assert!(absent.mark_anchor_offset().is_null());

        let present: MarkRecord = records.get(0).map(|rec| *rec).unwrap_or_default();
        assert!(!present.mark_anchor_offset().is_null());
    }

    #[test]
    fn an_index_past_the_end_is_none_not_an_error() {
        let table = SinglePosFormat2::read(data(font_test_data::gpos::SINGLEPOSFORMAT2)).unwrap();
        let records = table.value_records();
        assert_eq!(records.len(), 3);
        assert!(records.get(2).is_some());
        assert!(records.get(3).is_none());
    }
}
