#![parse_module(read_fonts::codegen_test::sanitize)]

extern scalar ValueFormat;

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

/// Table with version-conditional fields
table VersionedTable {
    #[version]
    version: MajorMinor,
    always_present: u16,
    #[since_version(1.1)]
    if_11_offset: Offset16<FlagTable>,
    #[since_version(2.0)]
    if_20: u32,
}

/// Flags for FlagTable
flags u8 FlagTableFlags {
    FOO = 0x01,
    BAR = 0x02,
}

/// Table with an array of scalars (no inner recursion needed)
table ScalarArrayTable {
    count: u16,
    #[count($count)]
    values: [u16],
}

/// Table with a nullable array of offsets
table NullableOffsetArrayTable {
    count: u16,
    #[count($count)]
    #[nullable]
    child_offsets: [Offset16<ScalarArrayTable>],
}

/// Table with a conditional array (flag-gated)
table ConditionalArrayTable {
    flags: FlagTableFlags,
    #[if_flag($flags, FlagTableFlags::FOO)]
    extra_count: u16,
    #[if_flag($flags, FlagTableFlags::FOO)]
    #[count($extra_count)]
    extra_values: [u16],
    #[if_flag($flags, FlagTableFlags::FOO)]
    another_field: u16,
}

/// Table with flag-conditional fields
table FlagTable {
    flags: FlagTableFlags,
    always_present: u16,
    #[if_flag($flags, FlagTableFlags::FOO)]
    if_foo: u16,
    #[if_flag($flags, FlagTableFlags::BAR)]
    if_bar: u16,
}

table HasComputedArray {
    version: u16,
    records_count: u16,
    format: ValueFormat,
    #[count($records_count)]
    #[read_with($format)]
    records: ComputedArray<ValueRecord>,
}

