// This file is a test input that can be rebuilt when making changes to the
// codegen tool itself.
//
// There is a separate codegen plan at resources/test_plan.toml that can be run
// to only rebuild the test outputs.

#![parse_module(read_fonts::codegen_test::records)]

table BasicTable {
    simple_count: u16,
    #[count($simple_count)]
    simple_records: [SimpleRecord],
    arrays_inner_count: u16,
    array_records_count: u32,
    #[count($array_records_count)]
    #[read_with($arrays_inner_count)]
    array_records: ComputedArray<ContainsArrays<'a>>,
}

record SimpleRecord {
    val1: u16,
    va2: u32,
}

#[read_args(array_len: u16)]
record ContainsArrays<'a> {
    #[count($array_len)]
    scalars: [u16],
    #[count($array_len)]
    records: [SimpleRecord],
}

record ContainsOffests {
    off_array_count: u16,
    #[read_offset_with($off_array_count)]
    array_offset: Offset16<[SimpleRecord]>,
    other_offset: Offset32<BasicTable>,
}
