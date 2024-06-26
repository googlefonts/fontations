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

pub mod offsets_arrays {
    include!("../generated/generated_test_offsets_arrays.rs");

    #[test]
    fn array_offsets() {
        let builder = crate::test_helpers::BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(12_u16) // offset to 0xdead
            .push(0u16) // nullable
            .push(2u16) // array len
            .push(12u16) // array offset
            .extend([0xdead_u16, 0xbeef]);

        let table = KindsOfOffsets::read(builder.font_data()).unwrap();
        assert_eq!(table.nonnullable().unwrap().value(), 0xdead);

        let array = table.array().unwrap();
        assert_eq!(array, &[0xdead, 0xbeef]);
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
    use font_types::MajorMinor;

    include!("../generated/generated_test_conditions.rs");

    #[test]
    fn majorminor_1() {
        let bytes = crate::test_helpers::BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0u16);
        let table = MajorMinorVersion::read(bytes.font_data()).unwrap();
        assert_eq!(table.always_present(), 0);
    }

    #[test]
    fn majorminor_1_1() {
        let bytes = crate::test_helpers::BeBuffer::new()
            .push(MajorMinor::VERSION_1_1)
            .push(0u16);
        // shouldn't parse, we're missing a field
        assert!(MajorMinorVersion::read(bytes.font_data()).is_err());

        let bytes = crate::test_helpers::BeBuffer::new()
            .push(MajorMinor::VERSION_1_1)
            .push(0u16)
            .push(1u16);
        let table = MajorMinorVersion::read(bytes.font_data()).unwrap();
        assert_eq!(table.if_11(), Some(1));
    }

    #[test]
    fn major_minor_2() {
        let bytes = crate::test_helpers::BeBuffer::new()
            .push(MajorMinor::VERSION_2_0)
            .push(0u16);
        // shouldn't parse, we're missing a field
        assert!(MajorMinorVersion::read(bytes.font_data()).is_err());

        let bytes = crate::test_helpers::BeBuffer::new()
            .push(MajorMinor::VERSION_2_0)
            .push(0u16)
            .push(2u32);
        let table = MajorMinorVersion::read(bytes.font_data()).unwrap();
        assert_eq!(table.if_11(), None);
        assert_eq!(table.if_20(), Some(2));
    }

    #[cfg(test)]
    fn make_flag_data(flags: GotFlags) -> crate::test_helpers::BeBuffer {
        let mut buf = crate::test_helpers::BeBuffer::new().push(42u16).push(flags);
        if flags.contains(GotFlags::FOO) {
            buf = buf.push(0xf00_u16);
        }
        if flags.contains(GotFlags::BAR) {
            buf = buf.push(0xba4_u16);
        }
        buf
    }

    #[test]
    fn flags_none() {
        let data = make_flag_data(GotFlags::empty());
        let table = FlagDay::read(data.font_data()).unwrap();
        assert!(table.foo().is_none());
        assert!(table.bar().is_none());
    }

    #[test]
    fn flags_foo() {
        let data = make_flag_data(GotFlags::FOO);
        let table = FlagDay::read(data.font_data()).unwrap();
        assert_eq!(table.foo(), Some(0xf00));
        assert!(table.bar().is_none());
    }

    #[test]
    fn flags_bar() {
        let data = make_flag_data(GotFlags::BAR);
        let table = FlagDay::read(data.font_data()).unwrap();
        assert!(table.foo().is_none());
        assert_eq!(table.bar(), Some(0xba4));
    }

    #[test]
    fn flags_foobar() {
        let data = make_flag_data(GotFlags::BAR | GotFlags::FOO);
        let table = FlagDay::read(data.font_data()).unwrap();
        assert_eq!(table.foo(), Some(0xf00));
        assert_eq!(table.bar(), Some(0xba4));
    }
}
