#![parse_module(read_fonts::codegen_test::read_args)]

#[read_args(mark_class_count: u16)]
table BaseArray {
    /// Number of BaseRecords
    #[compile(array_len($base_records))]
    base_count: u16,
    /// Array of BaseRecords, in order of baseCoverage Index.
    #[count($base_count)]
    #[read_with($mark_class_count)]
    base_records: ComputedArray<BaseRecord<'a>>
}

/// Part of [BaseArray]
#[read_args(mark_class_count: u16)]
record BaseRecord<'a> {
    /// Array of offsets (one per mark class) to Anchor tables. Offsets
    /// are from beginning of BaseArray table, ordered by class
    /// (offsets may be NULL).
    #[nullable]
    #[count($mark_class_count)]
    base_anchor_offsets: [u16],
}


