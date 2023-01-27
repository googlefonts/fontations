#![parse_module(read_fonts::tables::gvar)]

extern scalar TupleVariationCount;
extern record TupleVariationHeader;

/// The ['gvar' header](https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#gvar-header)
#[tag = "gvar"]
table Gvar {
    /// Major/minor version number of the glyph variations table — set to (1,0).
    version: MajorMinor,
    /// The number of variation axes for this font. This must be the
    /// same number as axisCount in the 'fvar' table.
    axis_count: u16,
    /// The number of shared tuple records. Shared tuple records can be
    /// referenced within glyph variation data tables for multiple
    /// glyphs, as opposed to other tuple records stored directly
    /// within a glyph variation data table.
    shared_tuple_count: u16,
    /// Offset from the start of this table to the shared tuple records.
    #[read_offset_with($shared_tuple_count, $axis_count)]
    shared_tuples_offset: Offset32<SharedTuples>,
    /// The number of glyphs in this font. This must match the number
    /// of glyphs stored elsewhere in the font.
    glyph_count: u16,
    /// Bit-field that gives the format of the offset array that
    /// follows. If bit 0 is clear, the offsets are uint16; if bit 0 is
    /// set, the offsets are uint32.
    flags: GvarFlags,
    /// Offset from the start of this table to the array of
    /// GlyphVariationData tables.
    glyph_variation_data_array_offset: u32,
    /// Offsets from the start of the GlyphVariationData array to each
    /// GlyphVariationData table.
    #[count($glyph_count)]
    #[read_with($flags)]
    #[traverse_with(skip)]
    glyph_variation_data_offsets: ComputedArray<U16Or32>,
}

flags u16 GvarFlags {
    /// If set, offsets to GlyphVariationData are 32 bits
    LONG_OFFSETS = 1,
}

/// Array of tuple records shared across all glyph variation data tables.
#[read_args(shared_tuple_count: u16, axis_count: u16)]
table SharedTuples {
    #[count($shared_tuple_count)]
    #[read_with($axis_count)]
    tuples: ComputedArray<Tuple<'a>>,
}

/// The [GlyphVariationData](https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#the-glyphvariationdata-table-array) table
table GlyphVariationDataHeader {
    /// A packed field. The high 4 bits are flags, and the low 12 bits
    /// are the number of tuple variation tables for this glyph. The
    /// number of tuple variation tables can be any number between 1
    /// and 4095.
    #[traverse_with(skip)]
    tuple_variation_count: TupleVariationCount,
    /// Offset from the start of the GlyphVariationData table to the
    /// serialized data
    #[traverse_with(skip)]
    serialized_data_offset: Offset16<FontData>,
    /// Array of tuple variation headers.
    #[count(..)]
    #[traverse_with(skip)]
    tuple_variation_headers: VarLenArray<TupleVariationHeader>,
}

