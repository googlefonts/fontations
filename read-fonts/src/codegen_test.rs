//! A module used to test codegen.
//!
//! This imports a single codegen output; while modifying the codegen crate,
//! this file can be regenerated to check that changes compile, without needing
//! to rebuild everything.
//!
//! To rebuild this input and test it, run:
//!
//! $ cargo run --bin=codegen resources/test_plan.toml && cargo test

pub mod offsets_arrays {
    include!("../generated/generated_test_offsets_arrays.rs");

    #[test]
    fn array_offsets() {
        let mut builder = crate::test_helpers::BeBuffer::new();
        builder.push(MajorMinor::VERSION_1_0);
        builder.push(12_u16); // offset to 0xdead
        builder.push(0u16); // nullable
        builder.push(2u16); // array len
        builder.push(12u16); // array offset
        builder.extend([0xdead_u16, 0xbeef]);

        let table = KindsOfOffsets::read(builder.font_data()).unwrap();
        assert_eq!(table.nonnullable().unwrap().value(), 0xdead);

        let array = table.array().unwrap();
        assert_eq!(array, &[0xdead, 0xbeef]);
    }
}
