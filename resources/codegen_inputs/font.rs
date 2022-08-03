#![parse_module(font_tables::font)]

table TableDirectory {
    sfnt_version: BigEndian<u32>,
    #[compile(array_len($table_records))]
    num_tables: BigEndian<u16>,
    search_range: BigEndian<u16>,
    entry_selector: BigEndian<u16>,
    range_shift: BigEndian<u16>,
    #[count(num_tables)]
    table_records: [ TableRecord ],
}

/// Record for a table in a font.
record TableRecord {
    /// Table identifier.
    tag: BigEndian<Tag>,
    /// Checksum for the table.
    checksum: BigEndian<u32>,
    /// Offset from the beginning of the font data.
    #[compile_type(u32)] // we set these manually
    offset: BigEndian<Offset32>,
    /// Length of the table.
    len: BigEndian<u32>,
}
