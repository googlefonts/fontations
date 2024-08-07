#![parse_module(read_fonts::tables::ankr)]

/// The [anchor point](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6ankr.html) table.
#[tag = "ankr"]
table Ankr {
    /// Version number (set to zero).
    version: u16,
    /// Flags (currently unused; set to zero).
    flags: u16,
    /// Offset to the table's lookup table; currently this is always `0x0000000C`.
    /// 
    /// Lookup values are two byte offsets into the glyph data table.
    lookup_table_offset: Offset32<LookupU16>,
    /// Offset to the glyph data table.
    glyph_data_table_offset: u32
}

table GlyphDataEntry {
    /// Number of anchor points for this glyph.
    num_points: u32,
    /// Individual anchor points.
    #[count($num_points)]
    anchor_points: [AnchorPoint],
}

/// Individual anchor point.
record AnchorPoint {
    x: i16,
    y: i16,
}
