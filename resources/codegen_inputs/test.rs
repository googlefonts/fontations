#![parse_module(read_fonts::codegen_test)]

table KindsOfOffsets {
    /// The major/minor version of the GDEF table
    #[version]
    version: BigEndian<MajorMinor>,
    /// A normal offset
    nonnullable_offset: BigEndian<Offset16<Dummy>>,
    /// An offset that is nullable, but always present
    #[nullable]
    nullable_offset: BigEndian<Offset16<Dummy>>,
    /// count of the array at array_offset
    array_offset_count: BigEndian<u16>,
    /// An offset to an array:
    #[read_offset_with($array_offset_count)]
    array_offset: BigEndian<Offset16<[BigEndian<u16>]>>,
    /// A normal offset that is versioned
    #[available(MajorMinor::VERSION_1_1)]
    versioned_nonnullable_offset: BigEndian<Offset16<Dummy>>,
    /// An offset that is nullable and versioned
    #[available(MajorMinor::VERSION_1_1)]
    #[nullable]
    versioned_nullable_offset: BigEndian<Offset16<Dummy>>,
}

table KindsOfArraysOfOffsets {
    /// The major/minor version of the GDEF table
    #[version]
    version: BigEndian<MajorMinor>,
    /// The number of items in each array
    count: BigEndian<u16>,
    /// A normal array offset
    #[count($count)]
    nonnullable_offsets: [BigEndian<Offset16<Dummy>>],
    /// An offset that is nullable, but always present
    #[nullable]
    #[count($count)]
    nullable_offsets: [BigEndian<Offset16<Dummy>>],
    /// A normal offset that is versioned
    #[available(MajorMinor::VERSION_1_1)]
    #[count($count)]
    versioned_nonnullable_offsets: [BigEndian<Offset16<Dummy>>],
    /// An offset that is nullable and versioned
    #[available(MajorMinor::VERSION_1_1)]
    #[nullable]
    #[count($count)]
    versioned_nullable_offsets: [BigEndian<Offset16<Dummy>>],
}

table Dummy {
    value: BigEndian<u16>,
}

