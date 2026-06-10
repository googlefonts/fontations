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
    fn sanitize_data(_ctx: &mut SanitizeContext) -> Result<(), ReadError> {
        Ok(())
    }

    impl HasOffsetsWithArgs {
        fn sanitize_fake_offset(&self, ctx: &mut SanitizeContext) -> Result<(), ReadError> {
            self.fake_offset().sanitize_offset::<HasReadArgs>(ctx, 0)
        }
    }

    impl HasOffsetsWithArgs {
        pub fn fake<'a>(&self, data: FontData<'a>) -> Result<HasReadArgs<'a>, ReadError> {
            self.fake_offset().resolve_with_args(data, &0)
        }
    }
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

    impl SanitizeStruct for VarSizeDummy<'_> {
        fn can_skip() -> bool {
            true
        }

        fn sanitize_struct(
            &self,
            _ctx: &mut SanitizeContext<'_>,
            _args: (),
        ) -> Result<(), ReadError> {
            Ok(())
        }
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
        // header (v1.0): MajorMinor(4) + nonnullable(2) + nullable(2) +
        //   count(2) + array_offset(2) + record_array_offset(2) = 14
        let builder = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(14_u16) // nonnullable → Dummy at offset 14
            .push(0u16) // nullable (null)
            .push(2u16) // array len
            .push(14u16) // array offset → data at 14
            .push(0u16) // record_array_offset (null)
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
        // Data is too small for the versioned fields — sanitize rejects it
        let buf = BeBuffer::new()
            .push(1u16) // version
            .push(1u16) // count
            .push(2u16) // scalar array
            .push(3u16)
            .push(4u32); // shmecord array
        assert!(KindsOfArrays::read(buf.data().into()).is_err());
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

    fn sanitize_remainder(_ctx: &mut SanitizeContext) -> Result<(), ReadError> {
        Ok(())
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
        // Too small for v1.1 — sanitize rejects it
        let bytes = BeBuffer::new().push(MajorMinor::VERSION_1_1).push(0u16);
        assert!(MajorMinorVersion::read(bytes.data().into()).is_err());

        let bytes = BeBuffer::new()
            .push(MajorMinor::VERSION_1_1)
            .push(0u16)
            .push(1u16);
        let table = MajorMinorVersion::read(bytes.data().into()).unwrap();
        assert_eq!(table.if_11(), Some(1));
    }

    #[test]
    fn major_minor_2() {
        // Too small for v2.0 — sanitize rejects it
        // v2.0 needs: MajorMinor(4) + always_present(2) + if_20(4) = 10
        // (if_11 is NOT present for v2.0 since compatible requires same major)
        let bytes = BeBuffer::new().push(MajorMinor::VERSION_2_0).push(0u16);
        assert!(MajorMinorVersion::read(bytes.data().into()).is_err());

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

pub mod generic_group {
    include!("../generated/generated_test_generic_group.rs");

    #[cfg(test)]
    use font_test_data::bebuffer::BeBuffer;

    /// Build bytes for a MyLookup with one subtable offset pointing at data
    /// immediately after the header.
    /// Layout: [lookup_type: u16, single_subtable_offset: Offset32,
    /// sub_table_count: u16, offset0: Offset16, ...subtable data]
    #[cfg(test)]
    fn make_lookup_with_format1(lookup_type: u16) -> BeBuffer {
        BeBuffer::new()
            .push(lookup_type) // lookup_type
            .push(0u32) // single_subtable_offset (null)
            .push(1u16) // sub_table_count
            .push(10u16) // offset to subtable (10 bytes from start)
            // subtable data (MySubtableFormat1): format=1, value=42
            .push(1u16)
            .push(42u16)
    }

    #[test]
    fn parse_lookup_group_type_one() {
        let buf = make_lookup_with_format1(1);
        let group = MyLookupGroup::read(buf.data().into()).unwrap();
        assert!(matches!(group, MyLookupGroup::TypeOne(_)));
        let lookup = group.of_unit_type();
        assert_eq!(lookup.lookup_type(), 1);
        assert_eq!(lookup.sub_table_count(), 1);
    }

    #[test]
    fn parse_lookup_group_type_two() {
        let buf = make_lookup_with_format1(2);
        let group = MyLookupGroup::read(buf.data().into()).unwrap();
        assert!(matches!(group, MyLookupGroup::TypeTwo(_)));
    }

    #[test]
    fn parse_lookup_group_invalid_type() {
        let buf = make_lookup_with_format1(99);
        let result = MyLookupGroup::read(buf.data().into());
        assert!(matches!(result, Err(ReadError::InvalidFormat(99))))
    }

    #[test]
    fn parse_subtable_format_dispatch() {
        // Format 1
        let buf = BeBuffer::new().push(1u16).push(42u16);
        let sub = MySubtable::read(buf.data().into()).unwrap();
        assert!(matches!(sub, MySubtable::Format1(_)));
        if let MySubtable::Format1(f1) = sub {
            assert_eq!(f1.value(), 42);
        }

        // Format 2
        let buf = BeBuffer::new()
            .push(2u16) // format
            .push(2u16) // count
            .extend([10u16, 20]);
        let sub = MySubtable::read(buf.data().into()).unwrap();
        assert!(matches!(sub, MySubtable::Format2(_)));
        if let MySubtable::Format2(f2) = sub {
            assert_eq!(f2.count(), 2);
        }
    }
}

#[cfg(test)]
mod sanitize_tests {
    use crate::{
        codegen_test::{offsets_arrays::*, records::*},
        sanitize::{Sanitize, SanitizeContext, SanitizeState},
        FontData, ReadError,
    };
    use font_test_data::bebuffer::BeBuffer;
    use font_types::MajorMinor;

    fn sanitize<T: Sanitize<Args = ()>>(data: &[u8]) -> Result<(), ReadError> {
        let mut state = SanitizeState::default();
        let mut ctx = SanitizeContext::new(FontData::new(data), &mut state);
        T::sanitize(&mut ctx, ())
    }

    // --- KindsOfOffsets (v1.0) layout: ---
    // MajorMinor(4) + nonnullable Offset16(2) + nullable Offset16(2) +
    // array_offset_count u16(2) + array_offset Offset16(2) + record_array_offset Offset16(2)
    // = 14 byte header

    /// A valid Dummy subtable (value: u16 + _reserved: u16 = 4 bytes)
    const DUMMY_BYTES: [u16; 2] = [0xdead, 0x0000];

    #[test]
    fn simple_offsets_valid() {
        // nonnullable → Dummy at offset 14, everything else null/zero
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(14u16) // nonnullable → Dummy right after header
            .push(0u16) // nullable (null)
            .push(0u16) // array_offset_count = 0
            .push(0u16) // array_offset (null, count is 0)
            .push(0u16) // record_array_offset (null, count is 0)
            // Dummy subtable
            .extend(DUMMY_BYTES);
        let result = sanitize::<KindsOfOffsets>(buf.data());
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn null_offset_is_not_an_error() {
        // All simple offsets null — sanitize skips null offsets
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0u16) // nonnullable (null — sanitize treats null as skip)
            .push(0u16) // nullable (null)
            .push(0u16) // array_offset_count = 0
            .push(0u16) // array_offset (null)
            .push(0u16); // record_array_offset (null)
        let result = sanitize::<KindsOfOffsets>(buf.data());
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn offset_out_of_bounds() {
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(9999u16) // nonnullable → way past end
            .push(0u16) // nullable (null)
            .push(0u16) // count = 0
            .push(0u16) // array_offset
            .push(0u16); // record_array_offset
        assert!(sanitize::<KindsOfOffsets>(buf.data()).is_err());
    }

    #[test]
    fn subtable_too_small() {
        // Offset points to valid position but only 2 bytes of data (Dummy needs 4)
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(14u16) // nonnullable → offset 14
            .push(0u16)
            .push(0u16)
            .push(0u16)
            .push(0u16)
            // Only 2 bytes — Dummy needs 4
            .push(0xdeadu16);
        assert!(sanitize::<KindsOfOffsets>(buf.data()).is_err());
    }

    #[test]
    fn offset_to_scalar_array_valid() {
        // array_offset_count = 2, array_offset → [u16; 2], record_array_offset null
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0u16) // nonnullable (null)
            .push(0u16) // nullable (null)
            .push(2u16) // array_offset_count = 2
            .push(14u16) // array_offset → right after header
            .push(0u16) // record_array_offset (null)
            // 2 u16 values
            .extend([0x1111u16, 0x2222]);
        assert!(sanitize::<KindsOfOffsets>(buf.data()).is_ok());
    }

    #[test]
    fn offset_to_record_array_valid() {
        // Shmecord = u16(2) + u32(4) = 6 bytes each
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0u16) // nonnullable (null)
            .push(0u16) // nullable (null)
            .push(1u16) // array_offset_count = 1
            .push(0u16) // array_offset (null)
            .push(14u16) // record_array_offset → right after header
            // 1 Shmecord
            .push(42u16)
            .push(99u32);
        assert!(sanitize::<KindsOfOffsets>(buf.data()).is_ok());
    }

    #[test]
    fn offset_to_array_count_overflows() {
        // count = 0xFFFF but only a few bytes of data
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0u16)
            .push(0u16)
            .push(0xFFFFu16) // array_offset_count = huge
            .push(14u16) // array_offset → valid position
            .push(0u16)
            // Only 4 bytes of data, not enough for 65535 u16s
            .extend([1u16, 2]);
        assert!(sanitize::<KindsOfOffsets>(buf.data()).is_err());
    }

    // --- KindsOfArraysOfOffsets (v1.0) layout: ---
    // MajorMinor(4) + count u16(2) + nonnullable_offsets [Offset16]*count +
    // nullable_offsets [Offset16]*count
    // (versioned fields skipped for v1.0)

    #[test]
    fn array_of_offsets_valid() {
        // 2 nonnullable offsets → valid Dummies, 2 nullable offsets (null)
        let header_size = 4 + 2 + 2 * 2 + 2 * 2; // = 14
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(2u16) // count
            // nonnullable offsets → Dummy at header_size and header_size+4
            .push(header_size as u16)
            .push((header_size + 4) as u16)
            // nullable offsets (both null)
            .push(0u16)
            .push(0u16)
            // Dummy 0
            .extend(DUMMY_BYTES)
            // Dummy 1
            .extend(DUMMY_BYTES);
        assert!(sanitize::<KindsOfArraysOfOffsets>(buf.data()).is_ok());
    }

    #[test]
    fn array_of_offsets_one_bad() {
        let header_size = 4 + 2 + 2 * 2 + 2 * 2; // = 14
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(2u16) // count
            // first offset valid, second out of bounds
            .push(header_size as u16)
            .push(0xFFFFu16)
            // nullable offsets (null)
            .push(0u16)
            .push(0u16)
            // Only one Dummy
            .extend(DUMMY_BYTES);
        assert!(sanitize::<KindsOfArraysOfOffsets>(buf.data()).is_err());
    }

    #[test]
    fn malformed_subtable_propagates() {
        // Offset points to a Dummy that's only 2 bytes (needs 4)
        let header_size = 4 + 2 + 1 * 2 + 1 * 2; // = 10
        let buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(1u16) // count = 1
            .push(header_size as u16) // nonnullable → offset 10
            .push(0u16) // nullable (null)
            // Truncated Dummy — only 2 bytes
            .push(0xdeadu16);
        assert!(sanitize::<KindsOfArraysOfOffsets>(buf.data()).is_err());
    }

    // --- BasicTable layout: ---
    // simple_count u16(2) + [SimpleRecord]*simple_count +
    // arrays_inner_count u16(2) + array_records_count u32(4) +
    // ComputedArray<ContainsArrays>
    //
    // SimpleRecord = u16(2) + u32(4) = 6 bytes
    // ContainsArrays(array_len=N) = [u16]*N + [SimpleRecord]*N

    #[test]
    fn computed_array_valid() {
        // simple_count=1, one SimpleRecord, arrays_inner_count=1,
        // array_records_count=1, one ContainsArrays with 1 scalar + 1 SimpleRecord
        let buf = BeBuffer::new()
            .push(1u16) // simple_count
            // SimpleRecord
            .push(1u16)
            .push(2u32)
            // arrays_inner_count
            .push(1u16)
            // array_records_count
            .push(1u32)
            // ContainsArrays { scalars: [u16; 1], records: [SimpleRecord; 1] }
            .push(42u16) // scalar
            .push(10u16) // SimpleRecord.val1
            .push(20u32); // SimpleRecord.va2
        assert!(sanitize::<BasicTable>(buf.data()).is_ok());
    }

    #[test]
    fn computed_array_truncated() {
        // array_records_count=1 but not enough data for the ContainsArrays
        let buf = BeBuffer::new()
            .push(0u16) // simple_count = 0
            .push(2u16) // arrays_inner_count = 2 (each ContainsArrays has 2 scalars + 2 records)
            .push(1u32) // array_records_count = 1
            // Only 2 bytes — not enough for ContainsArrays(2)
            .push(0u16);
        assert!(sanitize::<BasicTable>(buf.data()).is_err());
    }

    #[test]
    fn data_too_short_for_header() {
        // Not even enough bytes for the version field
        let buf = BeBuffer::new().push(0u16);
        assert!(sanitize::<KindsOfOffsets>(buf.data()).is_err());
    }
}
