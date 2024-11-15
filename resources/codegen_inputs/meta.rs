#![parse_module(read_fonts::tables::meta)]

/// [`meta`](https://docs.microsoft.com/en-us/typography/opentype/spec/meta)
#[tag = "meta"]
table Meta {
    /// Version number of the metadata table — set to 1.
    #[compile(1)]
    version: u32,
    /// Flags — currently unused; set to 0.
    #[compile(0)]
    flags: u32,
    /// Not used; set to 0.
    #[skip_getter]
    #[compile(0)]
    reserved: u32,
    /// The number of data maps in the table.
    #[compile(array_len($data_maps))]
    data_maps_count: u32,
    /// Array of data map records.
    #[count($data_maps_count)]
    data_maps: [DataMapRecord],
}

///  <https://learn.microsoft.com/en-us/typography/opentype/spec/meta#table-formats>
#[skip_from_obj]
record DataMapRecord {
    /// A tag indicating the type of metadata.
    tag: Tag,
    /// Offset in bytes from the beginning of the metadata table to the data for this tag.
    #[read_offset_with($tag, $data_length)]
    #[traverse_with(skip)]
    #[validate(validate_data_type)]
    data_offset: Offset32<Metadata>,
    /// Length of the data, in bytes. The data is not required to be padded to any byte boundary.
    #[compile(self.compute_data_len())]
    data_length: u32,
}

