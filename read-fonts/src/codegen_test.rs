//! A module used to test codegen.
//!
//! This imports a single codegen output; while modifying the codegen crate,
//! this file can be regenerated to check that changes compile, without needing
//! to rebuild everything.
//!
//! To rebuild this input and test it, run:
//!
//! $ cargo run --bin=codegen resources/test_plan.toml && cargo test

pub mod records {
    include!("../generated/generated_test_records.rs");
}

pub mod formats {
    include!("../generated/generated_test_formats.rs");
}

pub mod read_args {
    include!("../generated/generated_test_read_args.rs");
}

pub mod offsets_arrays {

    include!("../generated/generated_test_offsets_arrays.rs");

    #[cfg(test)]
    use font_test_data::bebuffer::BeBuffer;

    pub struct VarSizeDummy<'a> {
        #[allow(dead_code)]
        count: u16,
        pub bytes: &'a [u8],
    }

    impl VarSize for VarSizeDummy<'_> {
        type Size = u16;
    }

    impl<'a> FontRead<'a> for VarSizeDummy<'a> {
        fn read(data: FontData<'a>) -> Result<Self, ReadError> {
            let count: u16 = data.read_at(0)?;
            let bytes = data
                .as_bytes()
                .get(2..2 + (count as usize))
                .ok_or(ReadError::OutOfBounds)?;
            Ok(Self { count, bytes })
        }
    }

    #[test]
    fn array_offsets() {
        let builder = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(12_u16) // offset to 0xdead
            .push(0u16) // nullable
            .push(2u16) // array len
            .push(12u16) // array offset
            .extend([0xdead_u16, 0xbeef]);

        let table = KindsOfOffsets::read(builder.data().into()).unwrap();
        assert_eq!(table.nonnullable().unwrap().value(), 0xdead);

        let array = table.array().unwrap();
        assert_eq!(array, &[0xdead, 0xbeef]);
    }

    #[test]
    fn var_len_array_empty() {
        let builder = BeBuffer::new().push(0u16).push(0xdeadbeef_u32);

        let table = VarLenHaver::read(builder.data().into()).unwrap();
        assert_eq!(table.other_field(), 0xdeadbeef);
    }

    #[test]
    fn var_len_array_some() {
        let builder = BeBuffer::new()
            .push(3u16)
            .push(0u16) // first item in array is empty
            .push(2u16)
            .extend([1u8, 1])
            .push(5u16)
            .extend([7u8, 7, 7, 7, 7])
            .push(0xdeadbeef_u32);

        let table = VarLenHaver::read(builder.data().into()).unwrap();
        let kids = table
            .var_len()
            .iter()
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert!(kids[0].bytes.is_empty());
        assert_eq!(kids[1].bytes, &[1, 1]);
        assert_eq!(kids[2].bytes, &[7, 7, 7, 7, 7]);
        assert_eq!(table.other_field(), 0xdeadbeef)
    }

    #[test]
    #[cfg(feature = "experimental_traverse")]
    fn array_offsets_traverse() {
        let mut builder = BeBuffer::new()
            .push(MajorMinor::VERSION_1_1)
            .push(22_u16) // offset to [0xf00, 0xba4]
            .push(0u16) // nullable
            .push(2u16) // array len
            .push(26u16) // offset to [69, 70]
            .push(30u16) // record_array_offset
            .push(0u16) // versioned_nullable_record_array_offset
            .push(42u16) // versioned nonnullable offset
            .push(0u32); // versioned nullable offset
                         //
        let data_start = builder.len();
        assert_eq!(data_start, 22);
        builder = builder
            .extend([0xf00u16, 0xba4])
            .extend([69u16, 70])
            .push(3u16) // shmecord[0]
            .push(9u32)
            .push(5u16) // shmecord[1]
            .push(0xdead_beefu32)
            .extend([0xb01du16, 0xface]); // versioned nonnullable offset;

        let table = KindsOfOffsets::read(builder.data().into()).unwrap();
        // traversal should not crash
        let _ = format!("{table:?}");
        assert_eq!(
            table.versioned_nonnullable().unwrap().unwrap().value(),
            0xb01d
        );
    }

    #[test]
    fn versioned_array_bad_data() {
        let buf = BeBuffer::new()
            .push(1u16) // version
            .push(1u16) // count
            .push(2u16) // scalar array
            .push(3u16)
            .push(4u32); // shmecord array
        let table = KindsOfArrays::read(buf.data().into()).unwrap();
        assert!(table.versioned_scalars().is_none()); // should be there but isn't
    }
}

pub mod flags {
    include!("../generated/generated_test_flags.rs");

    #[test]
    fn basics() {
        let all = ValueFormat::all();
        let none = ValueFormat::empty();
        assert!(all.contains(ValueFormat::X_PLACEMENT));
        assert!(all.contains(ValueFormat::Y_PLACEMENT));
        assert!(!none.contains(ValueFormat::X_PLACEMENT));
        assert!(!none.contains(ValueFormat::Y_PLACEMENT));
        assert_eq!(none, ValueFormat::default());
    }

    #[test]
    fn formatting() {
        let all = ValueFormat::all();
        assert_eq!(format!("{all:?}"), "X_PLACEMENT | Y_PLACEMENT");
        let none = ValueFormat::empty();
        assert_eq!(format!("{none:?}"), "(empty)");
        let xplace = ValueFormat::X_PLACEMENT;
        assert_eq!(format!("{xplace:?}"), "X_PLACEMENT");
    }

    // not exactly a test, but this will fail to compile if these are missing
    #[test]
    fn impl_traits() {
        fn impl_check<T: Copy + std::hash::Hash + Eq + Ord>() {}
        impl_check::<ValueFormat>();
    }
}

pub mod enums {
    include!("../generated/generated_test_enum.rs");
}

pub mod count_all {
    use crate::FontData;

    include!("../generated/generated_test_count_all.rs");

    /// Test for count(..) with element sizes > 1
    #[test]
    fn element_size_greater_than_one_with_padding() {
        // Size of 13 ensures we have an extra padding byte
        let bytes = [0u8; 13];
        // Generated table has a 2 byte field above the array
        let remainder_len = bytes.len() - 2;
        let data = FontData::new(&bytes);
        // Trailing array with 16-bit elements
        assert!(remainder_len % 2 != 0);
        let count16 = CountAll16::read(data).unwrap();
        assert_eq!(count16.remainder().len(), remainder_len / 2);
        // Trailing array with 32-bit elements
        assert!(remainder_len % 4 != 0);
        let count32 = CountAll32::read(data).unwrap();
        assert_eq!(count32.remainder().len(), remainder_len / 4);
    }
}

pub mod conditions {
    #[cfg(test)]
    use font_test_data::bebuffer::BeBuffer;
    use font_types::MajorMinor;

    include!("../generated/generated_test_conditions.rs");

    #[test]
    fn majorminor_1() {
        let bytes = BeBuffer::new().push(MajorMinor::VERSION_1_0).push(0u16);
        let table = MajorMinorVersion::read(bytes.data().into()).unwrap();
        assert_eq!(table.always_present(), 0);
    }

    #[test]
    fn majorminor_1_1() {
        let bytes = BeBuffer::new().push(MajorMinor::VERSION_1_1).push(0u16);
        let too_small = MajorMinorVersion::read(bytes.data().into()).unwrap();
        // this is expected to be present but the data is malformed; we will
        // still parse the table but checked read of the field will fail
        assert!(too_small.if_11().is_none());

        let bytes = BeBuffer::new()
            .push(MajorMinor::VERSION_1_1)
            .push(0u16)
            .push(1u16);
        let table = MajorMinorVersion::read(bytes.data().into()).unwrap();
        assert_eq!(table.if_11(), Some(1));
    }

    #[test]
    fn major_minor_2() {
        let bytes = BeBuffer::new().push(MajorMinor::VERSION_2_0).push(0u16);
        let too_small = MajorMinorVersion::read(bytes.data().into()).unwrap();
        assert!(too_small.if_11().is_none());
        assert!(too_small.if_20().is_none());

        let bytes = BeBuffer::new()
            .push(MajorMinor::VERSION_2_0)
            .push(0u16)
            .push(2u32);
        let table = MajorMinorVersion::read(bytes.data().into()).unwrap();
        assert_eq!(table.if_11(), None);
        assert_eq!(table.if_20(), Some(2));
    }

    #[cfg(test)]
    fn make_flag_data(flags: GotFlags) -> BeBuffer {
        let mut buf = BeBuffer::new().push(42u16).push(flags);
        if flags.contains(GotFlags::FOO) {
            buf = buf.push(0xf00_u16);
        }
        if flags.contains(GotFlags::BAR) {
            buf = buf.push(0xba4_u16);
        }
        if flags.contains(GotFlags::FOO) || flags.contains(GotFlags::BAZ) {
            buf = buf.push(0xba2_u16);
        }
        buf
    }

    #[test]
    fn flags_none() {
        let data = make_flag_data(GotFlags::empty());
        let table = FlagDay::read(data.data().into()).unwrap();
        assert!(table.foo().is_none());
        assert!(table.bar().is_none());
    }

    #[test]
    fn flags_foo() {
        let data = make_flag_data(GotFlags::FOO);
        let table = FlagDay::read(data.data().into()).unwrap();
        assert_eq!(table.foo(), Some(0xf00));
        assert!(table.bar().is_none());
    }

    #[test]
    fn flags_bar() {
        let data = make_flag_data(GotFlags::BAR);
        let table = FlagDay::read(data.data().into()).unwrap();
        assert!(table.foo().is_none());
        assert_eq!(table.bar(), Some(0xba4));
    }

    #[test]
    fn flags_foobar() {
        let data = make_flag_data(GotFlags::BAR | GotFlags::FOO);
        let table = FlagDay::read(data.data().into()).unwrap();
        assert_eq!(table.foo(), Some(0xf00));
        assert_eq!(table.bar(), Some(0xba4));
    }
}

#[cfg(feature = "sanitize")]
pub mod sanitize {
    include!("../generated/generated_test_sanitize.rs");
    include!("../generated/generated_test_sanitize_sanitize.rs");

    #[cfg(test)]
    use font_test_data::bebuffer::BeBuffer;

    #[test]
    fn bad_array_of_offsets() {
        let buf = BeBuffer::new().push(1u16).push(1337u16).push(0xdeadu16);
        let table = GenericTable::<()>::read(buf.data().into()).unwrap();
        let table = table.into_concrete::<TableOne>();
        assert!(
            table.sanitize().is_err(),
            "not enough data for reported number of offsets"
        );
    }

    #[test]
    fn try_sanitize_table_one() {
        let buf = BeBuffer::new()
            .push(2u16) // record count
            .extend([10u16, 20]) // record 1
            .extend([30u16, 40]); // record 2
        let table = TableOne::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.record_count(), 2);
        let records = san.records();
        assert_eq!(records[0].ident(), 10);
        assert_eq!(records[0].derp(), 20);
        assert_eq!(records[1].ident(), 30);
        assert_eq!(records[1].derp(), 40);
    }

    #[test]
    fn table_one_not_enough_data() {
        let buf = BeBuffer::new()
            .push(13u16) // record count
            .extend([10u16, 20]) // record 1
            .extend([30u16, 40]); // record 2
                                  // we're missing lots more data tho

        let table = TableOne::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn try_sanitize_table_two_format1() {
        let buf = BeBuffer::new().push(1u16);
        let table = TableTwoFormat1::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.format(), 1);
    }

    #[test]
    fn try_sanitize_table_two_format2_null_child() {
        let buf = BeBuffer::new()
            .push(2u16) // format = 2
            .push(0u16); // child_offset = null
        let table = TableTwoFormat2::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.format(), 2);
        assert!(san.child().is_none());
    }

    #[test]
    fn try_sanitize_table_two_format2_with_child() {
        // TableTwoFormat2 header is 4 bytes (format u16 + child_offset u16),
        // so child_offset=4 points immediately after the header.
        let buf = BeBuffer::new()
            .push(2u16) // format = 2
            .push(4u16) // child_offset = 4
            // TableOne starts at offset 4
            .push(1u16) // record_count = 1
            .push(42u16) // records[0].ident
            .push(99u16); // records[0].derp
        let table = TableTwoFormat2::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.format(), 2);
        let child = san.child().unwrap();
        assert_eq!(child.record_count(), 1);
        let records = child.records();
        assert_eq!(records[0].ident(), 42);
        assert_eq!(records[0].derp(), 99);
    }

    #[test]
    fn try_sanitize_root_table() {
        // RootTable (6 bytes): MajorMinor + subtable_offset=6
        // GenericTable at byte 6 (6 bytes): type=1, count=1, offsets[0]=6
        // TableOne at byte 12: 2 records
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0) // version (4 bytes)
            .push(6u16) // subtable_offset -> GenericTable at byte 6
            // GenericTable at byte 6
            .push(1u16) // subtable_type = 1 (One/TableOne)
            .push(1u16) // subtable_count = 1
            .push(6u16) // subtable_offsets[0] = 6 (relative to GenericTable = byte 12)
            // TableOne at byte 12
            .push(2u16) // record_count = 2
            .push(10u16) // records[0].ident
            .push(20u16) // records[0].derp
            .push(30u16) // records[1].ident
            .push(40u16); // records[1].derp
        let table = RootTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.version(), MajorMinor::VERSION_1_0);
        let TableGroupSanitized::One(generic) = san.subtable() else {
            panic!("expected TableGroupSanitized::One");
        };
        assert_eq!(generic.subtable_type(), 1);
        assert_eq!(generic.subtable_count(), 1);
        let table_one = generic.subtables().get(0).unwrap();
        assert_eq!(table_one.record_count(), 2);
        let records = table_one.records();
        assert_eq!(records[0].ident(), 10);
        assert_eq!(records[0].derp(), 20);
        assert_eq!(records[1].ident(), 30);
        assert_eq!(records[1].derp(), 40);
    }

    #[test]
    fn sanitize_fails_record_count_out_of_bounds() {
        // record_count claims 100 records but only 1 record of data is present
        let buf = BeBuffer::new()
            .push(100u16) // record_count = 100 (far too many)
            .push(1u16) // records[0].ident
            .push(2u16); // records[0].derp
        let table = TableOne::read(buf.data().into()).unwrap();
        assert_eq!(table.sanitize(), Err(ReadError::InvalidArrayLen));
    }

    #[test]
    fn sanitize_fails_bad_child_offset() {
        // TableTwoFormat2 with a non-null but out-of-bounds child_offset
        let buf = BeBuffer::new()
            .push(2u16) // format = 2
            .push(0x7fffu16); // child_offset = way out of bounds
        let table = TableTwoFormat2::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn sanitize_fails_root_bad_subtable_offset() {
        // RootTable with an out-of-bounds subtable_offset
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0x7fffu16); // subtable_offset = way out of bounds
        let table = RootTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    // --- VersionedTable: version-conditional fields ---

    #[test]
    fn sanitize_versioned_table_1_0() {
        // v1.0: only always_present, conditional fields absent
        let buf = BeBuffer::new().push(MajorMinor::VERSION_1_0).push(42u16); // always_present
        let table = VersionedTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.always_present(), 42);
    }

    #[test]
    fn sanitize_versioned_table_1_1() {
        // v1.1: always_present + if_11, if_20 absent
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_1)
            .push(42u16) // always_present
            .push(8u16) // if_11 = table at pos 8
            .push(0u8) // flags table flags
            .push(303u16); // flags.always_present
        let table = VersionedTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.always_present(), 42);
        assert!(san.if_11_offset().is_some());
        let if11 = san.if_11().unwrap();
        assert_eq!(if11.always_present(), 303)
    }

    #[test]
    fn sanitize_versioned_table_2_0() {
        // v2.0: all fields present
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_2_0)
            .push(42u16) // always_present
            .push(99u16) // if_11
            .push(0xdeadbeefu32); // if_20
        let table = VersionedTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.always_present(), 42);
    }

    #[test]
    fn sanitize_versioned_table_fails_1_1_truncated() {
        // v1.1 declared but buffer missing if_11 — sanitize must fail
        let buf = BeBuffer::new().push(MajorMinor::VERSION_1_1).push(42u16); // always_present, no if_11
        let table = VersionedTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn sanitize_versioned_table_fails_2_0_truncated() {
        // v2.0 declared but buffer missing if_11 and if_20 — sanitize must fail
        let buf = BeBuffer::new().push(MajorMinor::VERSION_2_0).push(42u16); // always_present only
        let table = VersionedTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn missing_field_is_missing() {
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0xd00du16);
        let table = VersionedTable::read(buf.data().into()).unwrap();
        let sanitized = table.try_sanitize().unwrap();
        assert_eq!(sanitized.always_present(), 0xd00d);
        assert!(sanitized.if_11_offset().is_none());
        assert!(sanitized.if_11().is_none());
        assert!(sanitized.if_20().is_none());
    }
    // --- FlagTable: flag-conditional fields ---

    #[test]
    fn sanitize_flag_table_no_flags() {
        // No flags set: only always_present is present
        let buf = BeBuffer::new()
            .push(0u8) // flags = none
            .push(42u16); // always_present
        let table = FlagTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.always_present(), 42);
    }

    #[test]
    fn sanitize_flag_table_foo_only() {
        // FOO set: always_present + if_foo present
        let buf = BeBuffer::new()
            .push(FlagTableFlags::FOO)
            .push(42u16) // always_present
            .push(99u16); // if_foo
        let table = FlagTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.if_foo(), Some(99));
    }

    #[test]
    fn sanitize_flag_table_bar_only() {
        // FOO set: always_present + if_foo present
        let buf = BeBuffer::new()
            .push(FlagTableFlags::BAR)
            .push(42u16) // always_present
            .push(1234u16); // if_bar
        let table = FlagTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.if_bar_pos(), 3);
        assert_eq!(san.if_bar(), Some(1234));
    }
    #[test]
    fn sanitize_flag_table_both() {
        // FOO | BAR set: all fields present
        let buf = BeBuffer::new()
            .push(FlagTableFlags::FOO | FlagTableFlags::BAR) // flags = FOO | BAR
            .push(42u16) // always_present
            .push(99u16) // if_foo
            .push(77u16); // if_bar
        let table = FlagTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.always_present(), 42);
    }

    #[test]
    fn sanitize_flag_table_fails_foo_truncated() {
        // FOO set but no if_foo bytes — sanitize must fail
        let buf = BeBuffer::new()
            .push(0x01u8) // flags = FOO
            .push(42u16); // always_present, no if_foo
        let table = FlagTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn sanitize_flag_table_fails_bar_truncated() {
        // FOO | BAR set but no if_bar bytes — sanitize must fail
        let buf = BeBuffer::new()
            .push(0x03u8) // flags = FOO | BAR
            .push(42u16) // always_present
            .push(99u16); // if_foo, no if_bar
        let table = FlagTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    // --- ScalarArrayTable: array of plain scalars ---

    #[test]
    fn try_sanitize_scalar_array() {
        let buf = BeBuffer::new()
            .push(3u16) // count = 3
            .extend([10u16, 20, 30]); // values
        let table = ScalarArrayTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.count(), 3);
        let vals = san.values();
        assert_eq!(vals[0].get(), 10);
        assert_eq!(vals[1].get(), 20);
        assert_eq!(vals[2].get(), 30);
    }

    #[test]
    fn sanitize_scalar_array_bad_count() {
        let buf = BeBuffer::new()
            .push(100u16) // count = 100 (way too many)
            .extend([10u16, 20]); // only 2 values
        let table = ScalarArrayTable::read(buf.data().into()).unwrap();
        assert_eq!(table.sanitize(), Err(ReadError::InvalidArrayLen));
    }

    #[test]
    fn sanitize_scalar_array_empty() {
        let buf = BeBuffer::new().push(0u16); // count = 0
        let table = ScalarArrayTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.count(), 0);
        assert!(san.values().is_empty());
    }

    // --- NullableOffsetArrayTable: nullable array of offsets ---

    #[test]
    fn try_sanitize_nullable_offset_array_all_null() {
        let buf = BeBuffer::new()
            .push(2u16) // count = 2
            .push(0u16) // child_offsets[0] = null
            .push(0u16); // child_offsets[1] = null
        let table = NullableOffsetArrayTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.count(), 2);
        // All offsets are null, so iterating resolved children yields None
        for child in san.childs().iter() {
            assert!(child.is_none());
        }
    }

    #[test]
    fn try_sanitize_nullable_offset_array_mixed() {
        // Header: count(2) + 2 offsets = 6 bytes
        // ScalarArrayTable at offset 6: count(1) + value(42) = 4 bytes
        let buf = BeBuffer::new()
            .push(2u16) // count = 2
            .push(0u16) // child_offsets[0] = null
            .push(6u16) // child_offsets[1] = offset 6 (relative to table start)
            // ScalarArrayTable at byte 6
            .push(1u16) // count = 1
            .push(42u16); // values[0]
        let table = NullableOffsetArrayTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        let mut iter = san.childs().iter();
        assert!(iter.next().unwrap().is_none()); // null
        let child = iter.next().unwrap().unwrap(); // resolved
        assert_eq!(child.count(), 1);
        assert_eq!(child.values()[0].get(), 42);
    }

    #[test]
    fn sanitize_nullable_offset_array_bad_offset() {
        let buf = BeBuffer::new()
            .push(1u16) // count = 1
            .push(0x7fffu16); // child_offsets[0] = way out of bounds
        let table = NullableOffsetArrayTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn sanitize_nullable_offset_array_bad_count() {
        // count claims 1000 offsets but only 1 present
        let buf = BeBuffer::new().push(1000u16).push(0u16);
        let table = NullableOffsetArrayTable::read(buf.data().into()).unwrap();
        assert_eq!(table.sanitize(), Err(ReadError::InvalidArrayLen));
    }

    // --- ConditionalArrayTable: flag-gated arrays ---

    #[test]
    fn sanitize_conditional_array_no_flag() {
        // No flags: only the flags byte is present, all conditional fields absent
        let buf = BeBuffer::new().push(0u8); // flags = none
        let table = ConditionalArrayTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert!(san.extra_count().is_none());
        assert!(san.extra_values().is_none());
        assert!(san.another_field().is_none());
    }

    #[test]
    fn sanitize_conditional_array_with_flag() {
        // FOO set: extra_count + extra_values + another_field present
        // Layout: flags(1) + extra_count(2) + extra_values(2*2) + another_field(2) = 9 bytes
        let buf = BeBuffer::new()
            .push(FlagTableFlags::FOO) // flags = FOO
            .push(2u16) // extra_count = 2
            .extend([100u16, 200]) // extra_values
            .push(0xbeefu16); // another_field
        let table = ConditionalArrayTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.extra_count(), Some(2));
        let extra = san.extra_values().unwrap();
        assert_eq!(extra[0].get(), 100);
        assert_eq!(extra[1].get(), 200);
        assert_eq!(san.another_field(), Some(0xbeef));
    }

    #[test]
    fn sanitize_conditional_array_flag_truncated_count() {
        // FOO set but no extra_count bytes — sanitize must fail
        let buf = BeBuffer::new().push(FlagTableFlags::FOO); // flags = FOO, nothing else
        let table = ConditionalArrayTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn sanitize_conditional_array_flag_truncated_values() {
        // FOO set, extra_count claims 10 values but data is too short.
        // The read-fonts shape can't read_array for the claimed count,
        // so extra_values() returns None → MissingFieldForCondition.
        let buf = BeBuffer::new()
            .push(FlagTableFlags::FOO)
            .push(10u16) // extra_count = 10 (claims 20 bytes of values)
            .extend([1u16, 2]) // only 4 bytes of values
            .push(0u16); // another_field
        let table = ConditionalArrayTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn sanitize_conditional_array_range_check() {
        // FOO set, all fields present in shape, but sanitize range check
        // catches that extra_values extends beyond the actual data.
        // We need extra_values_byte_range to succeed in the read-fonts shape
        // but fail the sanitize range check. This happens when extra_count
        // is large enough that the byte range end exceeds offset_data().len().
        let buf = BeBuffer::new()
            .push(FlagTableFlags::FOO)
            .push(100u16) // extra_count = 100 (claims 200 bytes)
            .extend([1u16, 2]) // 4 bytes of values
            .push(0u16); // another_field
        let table = ConditionalArrayTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    #[test]
    fn sanitize_conditional_array_flag_missing_another_field() {
        // FOO set, extra_values present but another_field missing
        let buf = BeBuffer::new()
            .push(FlagTableFlags::FOO)
            .push(1u16) // extra_count = 1
            .push(42u16); // extra_values[0], no another_field
        let table = ConditionalArrayTable::read(buf.data().into()).unwrap();
        assert!(table.sanitize().is_err());
    }

    // --- Strengthen existing versioned table tests ---

    #[test]
    fn sanitize_versioned_table_1_0_conditional_fields_absent() {
        // v1.0: conditional fields absent — getters should return None
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0xd00du16);
        let table = VersionedTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.always_present(), 0xd00d);
        assert!(san.if_11_offset().is_none());
        assert!(san.if_11().is_none());
        assert!(san.if_20().is_none());
    }

    #[test]
    fn sanitize_versioned_table_1_1_verify_child() {
        // v1.1: if_11_offset present, resolves to FlagTable — verify child field values
        // Layout: MajorMinor(4) + always_present(2) + if_11_offset(2) = 8 bytes
        // FlagTable at byte 8: flags(1) + always_present(2) = 3 bytes
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_1)
            .push(42u16) // always_present
            .push(8u16) // if_11_offset -> FlagTable at byte 8
            // FlagTable at byte 8
            .push(0u8) // flags = none
            .push(303u16); // always_present
        let table = VersionedTable::read(buf.data().into()).unwrap();
        let san = table.try_sanitize().unwrap();
        assert_eq!(san.always_present(), 42);
        assert!(san.if_11_offset().is_some());
        let child = san.if_11().unwrap();
        assert_eq!(child.always_present(), 303);
        // v1.1 does not have if_20
        assert!(san.if_20().is_none());
    }
}
