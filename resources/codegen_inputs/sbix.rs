#![parse_module(read_fonts::tables::sbix)]

/// The [sbix (Standard Bitmap Graphics)](https://docs.microsoft.com/en-us/typography/opentype/spec/sbix) table
#[read_args(num_glyphs: u16)]
#[tag = "sbix"]
table Sbix {
    /// Table version number — set to 1.
    #[compile(1)]
    version: u16,
    /// Bit 0: Set to 1.
    /// Bit 1: Draw outlines.
    /// Bits 2 to 15: reserved (set to 0).
    flags: u16,
    /// Number of bitmap strikes.
    num_strikes: u32,
    /// Offsets from the beginning of the 'sbix' table to data for each individual bitmap strike.
    #[count($num_strikes)]
    #[read_offset_with($num_glyphs)]
    strike_offsets: [Offset32<Strike>],
}

/// [Strike](https://learn.microsoft.com/en-us/typography/opentype/spec/sbix#strikes) header table
#[read_args(num_glyphs: u16)]
table Strike {
    /// The PPEM size for which this strike was designed.
    ppem: u16,
    /// The device pixel density (in PPI) for which this strike was designed. (E.g., 96 PPI, 192 PPI.)
    ppi: u16,
    /// Offset from the beginning of the strike data header to bitmap data for an individual glyph ID.
    #[count(add($num_glyphs, 1))]
    glyph_data_offsets: [u32],
}

/// [Glyph data](https://learn.microsoft.com/en-us/typography/opentype/spec/sbix#glyph-data) table
table GlyphData {
    /// The horizontal (x-axis) position of the left edge of the bitmap graphic in relation to the glyph design space origin.
    origin_offset_x: i16,
    /// The vertical (y-axis) position of the bottom edge of the bitmap graphic in relation to the glyph design space origin.
    origin_offset_y: i16,
    /// Indicates the format of the embedded graphic data: one of 'jpg ', 'png ' or 'tiff', or the special format 'dupe'.
    graphic_type: Tag,
    /// The actual embedded graphic data. The total length is inferred from sequential entries in the glyphDataOffsets array and the fixed size (8 bytes) of the preceding fields.
    #[count(..)]
    data: [u8],
}
