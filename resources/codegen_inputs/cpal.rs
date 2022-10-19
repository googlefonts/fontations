/// [CPAL (Color Palette Table)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-table-header) table
table Cpal0 {
    /// Table version number (=0).
        version: BigEndian<u16>,
    /// Number of palette entries in each palette.
        num_palette_entries: BigEndian<u16>,
    /// Number of palettes in the table.
        num_palettes: BigEndian<u16>,
    /// Total number of color records, combined for all palettes.
        num_color_records: BigEndian<u16>,
    /// Offset from the beginning of CPAL table to the first 
    /// ColorRecord.
    color_records_array_offset: BigEndian<Offset32>,
    /// Index of each palette’s first color record in the combined 
    /// color record array.
    #[count($num_palettes)]
        color_record_indices: [BigEndian<u16>],
}

/// [CPAL (Color Palette Table)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-table-header) table
table Cpal1 {
    /// Table version number (=1).
        version: BigEndian<u16>,
    /// Number of palette entries in each palette.
        num_palette_entries: BigEndian<u16>,
    /// Number of palettes in the table.
        num_palettes: BigEndian<u16>,
    /// Total number of color records, combined for all palettes.
        num_color_records: BigEndian<u16>,
    /// Offset from the beginning of CPAL table to the first 
    /// ColorRecord.
    color_records_array_offset: BigEndian<Offset32>,
    /// Index of each palette’s first color record in the combined 
    /// color record array.
    #[count($num_palettes)]
        color_record_indices: [BigEndian<u16>],
    /// Offset from the beginning of CPAL table to the Palette Types 
    /// Array. Set to 0 if no array is provided.
    palette_types_array_offset: BigEndian<Offset32>,
    /// Offset from the beginning of CPAL table to the Palette Labels 
    /// Array. Set to 0 if no array is provided.
    palette_labels_array_offset: BigEndian<Offset32>,
    /// Offset from the beginning of CPAL table to the Palette Entry 
    /// Labels Array. Set to 0 if no array is provided.
    palette_entry_labels_array_offset: BigEndian<Offset32>,
}

/// [CPAL (Color Record)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-entries-and-color-records) record
record ColorRecords {
    /// Color records for all palettes
    #[count($num_color_records)]
    color_records: [ColorRecord],
}

/// [CPAL (Color Record)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-entries-and-color-records) record
record ColorRecord {
    /// Blue value (B0).
    blue: BigEndian<u8>,
    /// Green value (B1).
    green: BigEndian<u8>,
    ///     Red value (B2).
    red: BigEndian<u8>,
    /// Alpha value (B3).
    alpha: BigEndian<u8>,
}

/// [CPAL (Palette Type Array)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-type-array) record
record PaletteTypes {
    /// Array of 32-bit flag fields that describe properties of each 
    /// palette. See below for details.
    #[count($num_palettes)]
    palette_types: [BigEndian<u32>],
}

/// [CPAL (Palette Label Array)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-labels-array) record
record PaletteLabels {
    /// Array of 'name' table IDs (typically in the font-specific name 
    /// ID range) that specify user interface strings associated with 
    /// each palette. Use 0xFFFF if no name ID is provided for a 
    /// palette.
    #[count($num_palettes)]
    palette_labels: [BigEndian<u16>],
}

/// [CPAL (Palette Label Entry Array)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-entry-label-array) record
record PaletteLabelEntries {
    /// Array of 'name' table IDs (typically in the font-specific name 
    /// ID range) that specify user interface strings associated with 
    /// each palette entry, e.g. “Outline”, “Fill”. This set of 
    /// palette entry labels applies to all palettes in the font. Use 
    /// 0xFFFF if no name ID is provided for a palette entry.
    #[count($num_palette_entries)]
    palette_entry_labels: [BigEndian<u16>],
}

