// This file is a test input that can be rebuilt when making changes to the
// codegen tool itself.
//
// There is a separate codegen plan at resources/test_plan.toml that can be run
// to only rebuild the test outputs.

#![parse_module(read_fonts::codegen_test::offsets_arrays)]
#![sanitize]

#[skip_constructor]
table KindsOfOffsets {
    /// The major/minor version of the GDEF table
    #[version]
    #[default(MajorMinor::VERSION_1_1)]
    version: MajorMinor,
    /// A normal offset
    nonnullable_offset: Offset16<Dummy>,
    /// An offset that is nullable, but always present
    #[nullable]
    nullable_offset: Offset16<Dummy>,
    /// count of the array at array_offset
    #[compile(array_len($array_offset))]
    array_offset_count: u16,
    /// An offset to an array:
    #[read_offset_with($array_offset_count)]
    array_offset: Offset16<[u16]>,
    /// An offset to an array of records
    #[read_offset_with($array_offset_count)]
    record_array_offset: Offset16<[Shmecord]>,
    /// A nullable, versioned offset to an array of records
    #[read_offset_with($array_offset_count)]
    #[nullable]
    #[since_version(1.1)]
    versioned_nullable_record_array_offset: Offset16<[Shmecord]>,
    /// A normal offset that is versioned
    #[since_version(1.1)]
    versioned_nonnullable_offset: Offset16<Dummy>,
    /// An offset that is nullable and versioned
    #[since_version(1.1)]
    #[nullable]
    versioned_nullable_offset: Offset32<Dummy>,
}

#[skip_constructor]
table KindsOfArraysOfOffsets {
    /// The version
    #[version]
    #[compile(MajorMinor::VERSION_1_1)]
    version: MajorMinor,
    /// The number of items in each array
    #[compile(array_len($nonnullable_offsets))]
    count: u16,
    /// A normal array offset
    #[count($count)]
    nonnullable_offsets: [Offset16<Dummy>],
    /// An offset that is nullable, but always present
    #[nullable]
    #[count($count)]
    nullable_offsets: [Offset16<Dummy>],
    /// A normal offset that is versioned
    #[since_version(1.1)]
    #[count($count)]
    versioned_nonnullable_offsets: [Offset16<Dummy>],
    /// An offset that is nullable and versioned
    #[since_version(1.1)]
    #[nullable]
    #[count($count)]
    versioned_nullable_offsets: [Offset16<Dummy>],
}

#[skip_constructor]
table KindsOfArrays {
    #[version]
    #[default(1)]
    version: u16,
    /// the number of items in each array
    #[compile(array_len($scalars))]
    count: u16,
    /// an array of scalars
    #[count($count)]
    scalars: [u16],
    /// an array of records
    #[count($count)]
    records: [Shmecord],
    /// a versioned array of scalars
    #[since_version(1)]
    #[count($count)]
    versioned_scalars: [u16],
    /// a versioned array of scalars
    #[since_version(1)]
    #[count($count)]
    versioned_records: [Shmecord],
}

#[skip_constructor]
#[skip_sanitize]
table VarLenHaver {
    count: u16,
    #[count($count)]
    #[traverse_with(skip)]
    var_len: VarLenArray<VarSizeDummy>,
    other_field: u32,
}

#[skip_constructor]
table Dummy {
    value: u16,
    /// Set to 0.
    // If we didn't set compile(0) there would be no way for write-fonts to have a value.
    #[skip_getter]
    #[compile(0)]
    _reserved: u16,
    // Has no getter, but isn't a compile time const.
    // write-fonts users need to set this themselves.
    // #[skip_getter]
    // #[user_computed]
    // offset: u32,
}

#[skip_constructor]
record Shmecord {
    length: u16,
    breadth: u32,
}
