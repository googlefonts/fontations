#![parse_module(read_fonts::codegen_test::read_args)]
#![sanitize]

#[read_args(mark_class_count: u16)]
#[skip_constructor]
table BaseArray {
    /// Number of BaseRecords
    #[compile(array_len($base_records))]
    base_count: u16,
    /// Array of BaseRecords, in order of baseCoverage Index.
    #[count($base_count)]
    #[read_with($mark_class_count)]
    #[sanitize_len_only]
    base_records: ComputedArray<BaseRecord<'a>>,
    #[compile(array_len($face_records))]
    face_count: u16,
    #[count($face_count)]
    #[read_with($mark_class_count)]
    face_records: ComputedArray<FaceRecord<'a>>,

}

/// Contains a scalar array
#[read_args(mark_class_count: u16)]
#[skip_constructor]
record BaseRecord<'a> {
    /// Array of offsets (one per mark class) to Anchor tables. Offsets
    /// are from beginning of BaseArray table, ordered by class
    /// (offsets may be NULL).
    #[nullable]
    #[count($mark_class_count)]
    base_anchor_offsets: [u16],
}


/// Contains offsets
#[read_args(mark_class_count: u16)]
#[skip_constructor]
record FaceRecord<'a> {
    #[nullable]
    #[count($mark_class_count)]
    face_offsets: [Offset16<Face>],
}

#[skip_constructor]
table Face {
    field: u16,
}
