#![parse_module(read_fonts::tables::head)]

/// The `macStyle` field for the head table.
flags u16 MacStyle {
    /// Bit 0: Bold (if set to 1)
    BOLD = 0x0001,
    /// Bit 1: Italic (if set to 1)
    ITALIC = 0x0002,
    /// Bit 2: Underline (if set to 1)
    UNDERLINE = 0x0004,
    /// Bit 3: Outline (if set to 1)
    OUTLINE = 0x0008,
    /// Bit 4: Shadow (if set to 1)
    SHADOW = 0x0010,
    /// Bit 5: Condensed (if set to 1)
    CONDENSED = 0x0020,
    /// Bit 6: Extended (if set to 1)
    EXTENDED = 0x0040,
    // Bits 7-15: Reserved (set to 0)    
}

/// The [head](https://docs.microsoft.com/en-us/typography/opentype/spec/head) 
/// (font header) table.
#[tag = "head"]
table Head {
    /// Version number of the font header table, set to (1, 0)
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Set by font manufacturer.
    font_revision: Fixed,
    /// To compute: set it to 0, sum the entire font as uint32, then
    /// store 0xB1B0AFBA - sum. If the font is used as a component in a
    /// font collection file, the value of this field will be
    /// invalidated by changes to the file structure and font table
    /// directory, and must be ignored.
    checksum_adjustment: u32,
    /// Set to 0x5F0F3CF5.
    #[default(0x5F0F3CF5)]
    magic_number: u32,
    /// See the flags enum
    flags: u16,
    /// Set to a value from 16 to 16384. Any value in this range is
    /// valid. In fonts that have TrueType outlines, a power of 2 is
    /// recommended as this allows performance optimizations in some
    /// rasterizers.
    units_per_em: u16,
    /// Number of seconds since 12:00 midnight that started January 1st
    /// 1904 in GMT/UTC time zone.
    created: LongDateTime,
    /// Number of seconds since 12:00 midnight that started January 1st
    /// 1904 in GMT/UTC time zone.
    modified: LongDateTime,
    /// Minimum x coordinate across all glyph bounding boxes.
    x_min: i16,
    /// Minimum y coordinate across all glyph bounding boxes.
    y_min: i16,
    /// Maximum x coordinate across all glyph bounding boxes.
    x_max: i16,
    /// Maximum y coordinate across all glyph bounding boxes.
    y_max: i16,
    /// see somewhere else
    mac_style: MacStyle,
    /// Smallest readable size in pixels.
    lowest_rec_ppem: u16,
    /// Deprecated (Set to 2).
    #[default(2)]
    font_direction_hint: i16,
    /// 0 for short offsets (Offset16), 1 for long (Offset32).
    index_to_loc_format: i16,
    /// 0 for current format.
    #[compile(0)]
    glyph_data_format: i16,
}
