#![parse_module(read_fonts::codegen_test::sanitize)]

// what are some things I want?
//
// - arrays
// - arrays of offsets
// - groups
// - format groups

table RootTable {
    #[version]
    version: MajorMinor,
    subtable_offset: Offset16<TableGroup>,
}

#[generic_offset(T)]
table GenericTable {
    subtable_type: u16,
    subtable_count: u16,
    #[count($subtable_count)]
    subtable_offsets: [Offset16<T>],
}

group TableGroup(GenericTable, $subtable_type) {
    1 => One(TableOne),
    2 => Two(TableTwo),
}

table TableOne {
    record_count: u16,
    #[count($record_count)]
    records: [TestRecord],
}

record TestRecord {
    ident: u16,
    derp: u16,
}

table TableTwoFormat1 {
    #[format = 1]
    format: u16,
}

table TableTwoFormat2 {
    #[format = 2]
    format: u16,
    #[nullable]
    child_offset: Offset16<TableOne>,
}

format u16 TableTwo {
    Format1(TableTwoFormat1),
    Format2(TableTwoFormat2),
}
