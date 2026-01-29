// This file is a test input that can be rebuilt when making changes to the
// codegen tool itself.
//
// There is a separate codegen plan at resources/test_plan.toml that can be run
// to only rebuild the test outputs.

#![parse_module(read_fonts::codegen_test::formats)]
#![sanitize]

table Table1 {
    #[format = 1]
    format: u16,
    heft: u32,
    flex: u16,
}

table Table2 {
    #[format = 2]
    format: u16,
    #[compile(array_len($values))]
    value_count: u16,
    #[count($value_count)]
    values: [u16],
}

#[skip_constructor]
table Table3 {
    #[format = 3]
    format: u16,
    something: u16,
}

format u16 MyTable {
    Format1(Table1),
    //constructor should be my_format_22
    MyFormat22(Table2),
    // I should get no constructor
    Format3(Table3),
}

table HostTable {
    child_offset: Offset16<MyTable>,
}
