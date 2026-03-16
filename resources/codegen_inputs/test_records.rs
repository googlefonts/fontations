// This file is a test input that can be rebuilt when making changes to the
// codegen tool itself.
//
// There is a separate codegen plan at resources/test_plan.toml that can be run
// to only rebuild the test outputs.

#![parse_module(read_fonts::codegen_test::records)]

#[validate(my_custom_validate)]
table BasicTable {
    #[compile(array_len($simple_records))]
    simple_count: u16,
    #[count($simple_count)]
    simple_records: [SimpleRecord],
    #[compile(self.compute_arrays_inner_count())]
    arrays_inner_count: u16,
    #[compile(array_len($array_records))]
    array_records_count: u32,
    #[count($array_records_count)]
    #[read_with($arrays_inner_count)]
    array_records: ComputedArray<ContainsArrays<'a>>,
}

record SimpleRecord {
    val1: u16,
    #[compile_with(compile_va2)]
    va2: u32,
}

#[read_args(array_len: u16)]
record ContainsArrays<'a> {
    #[count($array_len)]
    scalars: [u16],
    #[count($array_len)]
    records: [SimpleRecord],
}

record ContainsOffsets {
    #[compile(array_len($array_offset))]
    off_array_count: u16,
    #[read_offset_with($off_array_count)]
    #[offset_from(BasicTable)]
    array_offset: Offset16<[SimpleRecord]>,
    #[offset_from(BasicTable)]
    other_offset: Offset32<BasicTable>,
}

#[skip_constructor]
table VarLenItem {
    length: u32,
    #[count(..)]
    data: [u8],
}
