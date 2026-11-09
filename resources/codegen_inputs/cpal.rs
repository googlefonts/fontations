#![parse_module(read_fonts::tables::cpal)]

/// [CPAL (Color Palette Table)](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-table-header) table
#[tag = "CPAL"]
table Cpal {
    /// Table version number (=0).
    #[version]
    #[compile(0)]
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
    #[since_version(1)]
    #[nullable]
    #[read_offset_with($num_palettes)]
    palette_types_array_offset: Offset32<[PaletteType]>,
    /// Offset from the beginning of CPAL table to the [Palette Labels Array][].
    ///
    /// This is an array of 'name' table IDs (typically in the font-specific name
    /// ID range) that specify user interface strings associated with  each palette.
    /// Use 0xFFFF if no name ID is provided for a palette.
    ///
    /// [Palette Labels Array]: https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-labels-array
    #[since_version(1)]
    #[nullable]
    #[read_offset_with($num_palettes)]
    palette_labels_array_offset: Offset32<[NameId]>,
    /// Offset from the beginning of CPAL table to the [Palette Entry Labels Array][].
    ///
    /// This is an array of 'name' table IDs (typically in the font-specific name
    /// ID range) that specify user interface strings associated with  each palette
    /// entry, e.g. “Outline”, “Fill”. This set of palette entry labels applies
    /// to all palettes in the font. Use  0xFFFF if no name ID is provided for a
    /// palette entry.
    ///
    /// [Palette Entry Labels Array]: https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-entry-label-array
    #[since_version(1)]
    #[nullable]
    #[read_offset_with($num_palette_entries)]
    palette_entry_labels_array_offset: Offset32<[NameId]>,
}

/// Flags describing the backgrounds for which a palette is suitable.
///
/// These flags help applications select a palette for the intended background.
/// Both flags may be set when a palette is suitable for both light and dark
/// backgrounds. All other bits are reserved and must be zero.
///
/// Palette type information is optional and is available in CPAL version 1.
/// See the [CPAL palette type specification](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-type-array).
flags u32 PaletteType {
    /// The palette is suitable for displaying text on a light background, such as white.
    USABLE_WITH_LIGHT_BACKGROUND = 0x0001,
    /// The palette is suitable for displaying text on a dark background, such as black.
    USABLE_WITH_DARK_BACKGROUND = 0x0002,
}

/// A palette color stored as four 8-bit components.
///
/// The RGB components use the non-linear sRGB encoding and are not premultiplied
/// by alpha. Alpha represents opacity on a linear scale: 0 is fully transparent
/// and 255 is fully opaque.
///
/// See the [CPAL color record specification](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal#palette-entries-and-color-records).
record ColorRecord {
    /// Blue component in non-linear sRGB, from 0 to 255.
    blue: u8,
    /// Green component in non-linear sRGB, from 0 to 255.
    green: u8,
    /// Red component in non-linear sRGB, from 0 to 255.
    red: u8,
    /// Opacity, from 0 (fully transparent) to 255 (fully opaque).
    ///
    /// This may be combined with additional opacity specified by the paint
    /// using this color, such as the alpha value in a COLR paint table.
    alpha: u8,
}
