#![parse_module(read_fonts)]

/// The OpenType [Table Directory](https://docs.microsoft.com/en-us/typography/opentype/spec/otff#table-directory)
#[skip_from_obj]
table TableDirectory {
    /// 0x00010000 or 0x4F54544F
    sfnt_version: u32,
    /// Number of tables.
    #[compile(array_len($table_records))]
    num_tables: u16,
    search_range: u16,
    entry_selector: u16,
    range_shift: u16,
    /// Table records array—one for each top-level table in the font
    #[count($num_tables)]
    table_records: [ TableRecord ],
}

/// Record for a table in a font.
#[skip_from_obj]
record TableRecord {
    /// Table identifier.
    tag: Tag,
    /// Checksum for the table.
    checksum: u32,
    /// Offset from the beginning of the font data.
    // we handle this offset manually, since we can't always know the type
    offset: u32,
    /// Length of the table.
    length: u32,
}

/// [TTC Header](https://learn.microsoft.com/en-us/typography/opentype/spec/otff#ttc-header)
#[skip_from_obj]
#[skip_font_write]
#[skip_constructor]
table TTCHeader {
    /// Font Collection ID string: "ttcf"
    ttc_tag: Tag,
    /// Major/minor version of the TTC Header
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,
    /// Number of fonts in TTC
    num_fonts: u32,
    /// Array of offsets to the TableDirectory for each font from the beginning of the file
    #[count($num_fonts)]
    table_directory_offsets: [u32],

    /// Tag indicating that a DSIG table exists, 0x44534947 ('DSIG') (null if no signature)
    #[since_version(2.0)]
    dsig_tag: u32,
    /// The length (in bytes) of the DSIG table (null if no signature)
    #[since_version(2.0)]
    dsig_length: u32,
    /// The offset (in bytes) of the DSIG table from the beginning of the TTC file (null if no signature)
    #[since_version(2.0)]
    dsig_offset: u32,
}
