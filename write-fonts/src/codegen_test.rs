//! A module used to test codegen.
//!
//! This imports a single codegen output; while modifying the codegen crate,
//! this file can be regenerated to check that changes compile, without needing
//! to rebuild everything.
//!
//! To rebuild this input and test it, run:
//!
//! $ cargo run --bin=codegen resources/test_plan.toml && cargo test

mod records {
    include!("../generated/generated_test_records.rs");

    impl BasicTable {
        fn compute_arrays_inner_count(&self) -> u16 {
            self.array_records
                .first()
                .map(|x| x.scalars.len().try_into().unwrap())
                .unwrap_or_default()
        }
    }

    #[test]
    fn constructors() {
        let simple = vec![SimpleRecord::new(6, 32)];
        let contains_arrays = ContainsArrays::new(vec![1, 2, 3], simple.clone());
        let basic = BasicTable::new(simple.clone(), vec![contains_arrays]);
        let contains_offsets = ContainsOffests::new(simple, basic);
        assert_eq!(contains_offsets.other.simple_records.len(), 1);
    }
}

mod offsets_arrays {
    include!("../generated/generated_test_offsets_arrays.rs");
}
