#![parse_module(read_fonts::tables::cpal)]

/// [CPAL (Color Palette Table)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-table-header) table
table Cpal {
    /// Table version number (=0).
    #[version]
    version: u16,
    /// Number of palette entries in each palette.
    num_palette_entries: u16,
    /// Number of palettes in the table.
    num_palettes: u16,
    /// Total number of color records, combined for all palettes.
    num_color_records: u16,
    /// Offset from the beginning of CPAL table to the first
    /// ColorRecord.
    #[nullable]
    #[read_offset_with($num_color_records)]
    color_records_array_offset: Offset32<[ColorRecord]>,
    /// Index of each palette’s first color record in the combined
    /// color record array.
    #[count($num_palettes)]
    color_record_indices: [u16],

    /// Offset from the beginning of CPAL table to the [Palette Types Array][].
    ///
    /// This is an array of 32-bit flag fields that describe properties of each palette.
    ///
    /// [Palette Types Array]: https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-type-array
    #[available(1)]
    #[nullable]
    #[read_offset_with($num_palettes)]
    palette_types_array_offset: Offset32<[u32]>,
    /// Offset from the beginning of CPAL table to the [Palette Labels Array][].
    ///
    /// This is an array of 'name' table IDs (typically in the font-specific name
    /// ID range) that specify user interface strings associated with  each palette.
    /// Use 0xFFFF if no name ID is provided for a palette.
    ///
    /// [Palette Labels Array]: https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-labels-array
    #[available(1)]
    #[nullable]
    #[read_offset_with($num_palettes)]
    palette_labels_array_offset: Offset32<[u16]>,
    /// Offset from the beginning of CPAL table to the [Palette Entry Labels Array][].
    ///
    /// This is an array of 'name' table IDs (typically in the font-specific name
    /// ID range) that specify user interface strings associated with  each palette
    /// entry, e.g. “Outline”, “Fill”. This set of palette entry labels applies
    /// to all palettes in the font. Use  0xFFFF if no name ID is provided for a
    /// palette entry.
    ///
    /// [Palette Entry Labels Array]: https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-entry-label-array
    #[available(1)]
    #[nullable]
    #[read_offset_with($num_palette_entries)]
    palette_entry_labels_array_offset: Offset32<[u16]>,
}

/// [CPAL (Color Record)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-entries-and-color-records) record
record ColorRecord {
    /// Blue value (B0).
    blue: u8,
    /// Green value (B1).
    green: u8,
    ///     Red value (B2).
    red: u8,
    /// Alpha value (B3).
    alpha: u8,
}
