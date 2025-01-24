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

        fn my_custom_validate(&self, _: &mut ValidationCtx) {
            // Wowee, I can validate the entire table!
        }
    }

    impl SimpleRecord {
        // the [compile_with] attribute specifies this method, which returns
        // a different type than the one declared in the table.
        fn compile_va2(&self) -> [u8; 4] {
            [0xde, 0xad, 0xbe, 0xef]
        }
    }

    #[test]
    fn compile_with() {
        let record = SimpleRecord::new(69, 16);
        let bytes = crate::dump_table(&record).unwrap();
        assert_eq!(bytes, [0, 69, 0xde, 0xad, 0xbe, 0xef])
    }

    #[test]
    fn constructors() {
        let simple = vec![SimpleRecord::new(6, 32)];
        let contains_arrays = ContainsArrays::new(vec![1, 2, 3], simple.clone());
        let basic = BasicTable::new(simple.clone(), vec![contains_arrays]);
        let contains_offsets = ContainsOffsets::new(simple, basic);
        assert_eq!(contains_offsets.other.simple_records.len(), 1);
    }
}

mod formats {

    include!("../generated/generated_test_formats.rs");

    #[test]
    fn construct_formats() {
        let one = MyTable::format_1(404, 12);
        let two = MyTable::my_format_22(vec![5, 6, 7]);

        assert!(matches!(
            one,
            MyTable::Format1(Table1 {
                heft: 404,
                flex: 12
            })
        ));
        assert!(matches!(two, MyTable::MyFormat22(Table2 { .. })));
    }
}

mod offsets_arrays {
    include!("../generated/generated_test_offsets_arrays.rs");
    #[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct VarSizeDummy {
        bytes: Vec<u8>,
    }

    impl Validate for VarSizeDummy {
        fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
    }

    impl FontWrite for VarSizeDummy {
        fn write_into(&self, writer: &mut TableWriter) {
            (self.bytes.len() as u16).write_into(writer);
            self.bytes.write_into(writer);
        }
    }

    impl FromObjRef<read_fonts::codegen_test::offsets_arrays::VarSizeDummy<'_>> for VarSizeDummy {
        fn from_obj_ref(
            from: &read_fonts::codegen_test::offsets_arrays::VarSizeDummy,
            _: FontData,
        ) -> Self {
            Self {
                bytes: from.bytes.to_owned(),
            }
        }
    }
}

mod enums {
    include!("../generated/generated_test_enum.rs");

    #[test]
    fn default_works() {
        let rec = MyRecord::new(Default::default(), Default::default());
        assert_eq!(MyEnum1::ItsAZero, rec.my_enum1);
        assert_eq!(MyEnum2::ItsAThree, rec.my_enum2);
    }
}

pub mod conditions {
    #[cfg(test)]
    use font_test_data::bebuffer::BeBuffer;

    include!("../generated/generated_test_conditions.rs");

    #[test]
    #[should_panic(expected = "'foo' is present but FOO not set")]
    fn field_present_flag_missing() {
        let mut flags_are_wrong = FlagDay::new(42, GotFlags::empty());
        flags_are_wrong.foo = Some(0xf00);
        crate::dump_table(&flags_are_wrong).unwrap();
    }

    #[test]
    #[should_panic(expected = "if_cond is not satisfied but 'baz' is present.")]
    fn field_present_flag_missing_any_flag() {
        let mut flags_are_wrong = FlagDay::new(42, GotFlags::empty());
        flags_are_wrong.baz = Some(0xf00);
        crate::dump_table(&flags_are_wrong).unwrap();
    }

    #[test]
    #[should_panic(expected = "FOO is set but 'foo' is None")]
    fn flag_present_field_missing() {
        let flags_are_wrong = FlagDay::new(42, GotFlags::FOO);
        crate::dump_table(&flags_are_wrong).unwrap();
    }

    #[test]
    #[should_panic(expected = "if_cond is satisfied by 'baz' is not present.")]
    fn flag_present_field_missing_any_flags() {
        let flags_are_wrong = FlagDay::new(42, GotFlags::BAZ);
        crate::dump_table(&flags_are_wrong).unwrap();
    }

    #[test]
    fn fields_after_conditions_empty() {
        let table = FieldsAfterConditionals::new(GotFlags::empty(), 1, 2, 3);
        let data = crate::dump_table(&table).unwrap();
        let expected = BeBuffer::new().extend([0u16, 1, 2, 3]);
        assert_eq!(expected.as_slice(), data);
    }

    #[test]
    fn fields_after_conditions_one_present() {
        let mut table = FieldsAfterConditionals::new(GotFlags::BAR, 1, 2, 3);
        table.bar = Some(0xba4);
        let data = crate::dump_table(&table).unwrap();
        let expected = BeBuffer::new()
            .push(GotFlags::BAR)
            .extend([1u16, 0xba4, 2, 3]);
        assert_eq!(expected.as_slice(), data);
    }

    #[test]
    fn fields_after_conditions_all_present() {
        let mut table =
            FieldsAfterConditionals::new(GotFlags::BAR | GotFlags::BAZ | GotFlags::FOO, 1, 2, 3);
        table.bar = Some(0xba4);
        table.foo = Some(0xf00);
        table.baz = Some(0xba2);
        let data = crate::dump_table(&table).unwrap();
        let expected = BeBuffer::new()
            .push(GotFlags::BAR | GotFlags::BAZ | GotFlags::FOO)
            .extend([0xf00u16, 1, 0xba4, 0xba2, 2, 3]);
        assert_eq!(expected.as_slice(), data);
    }
}
