#![parse_module(read_fonts)]

/// The OpenType [Table Directory](https://docs.microsoft.com/en-us/typography/opentype/spec/otff#table-directory)
#[skip_from_obj]
table TableDirectory {
    /// 0x00010000 or 0x4F54544F
    sfnt_version: BigEndian<u32>,
    /// Number of tables.
    #[compile(array_len($table_records))]
    num_tables: BigEndian<u16>,
    search_range: BigEndian<u16>,
    entry_selector: BigEndian<u16>,
    range_shift: BigEndian<u16>,
    /// Table records array—one for each top-level table in the font
    #[count($num_tables)]
    table_records: [ TableRecord ],
}

/// Record for a table in a font.
#[skip_from_obj]
record TableRecord {
    /// Table identifier.
    tag: BigEndian<Tag>,
    /// Checksum for the table.
    checksum: BigEndian<u32>,
    /// Offset from the beginning of the font data.
    #[compile_type(u32)] // we set these manually
    offset: BigEndian<Offset32>,
    /// Length of the table.
    length: BigEndian<u32>,
}
