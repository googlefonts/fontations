#![parse_module(read_fonts::tables::kern)]

extern record Subtable0Pair;

/// The Apple Advanced Typography [kerning](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6kern.html) table.
table AatKern {
    /// The version number of the kerning table (0x00010000 for the current version).
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// The number of subtables included in the kerning table.
    n_tables: u32,
    /// Data for subtables, immediately following the header.    
    #[count(..)]
    subtable_data: [u8],
}

/// The OpenType [kerning](https://learn.microsoft.com/en-us/typography/opentype/spec/kern) table.
table OtKern {
    /// Table version number—set to 0.
    #[compile(0)]
    version: u16,
    /// Number of subtables in the kerning table.
    n_tables: u16,
    /// Data for subtables, immediately following the header.
    #[count(..)]
    subtable_data: [u8],
}

/// A subtable in an AAT `kern` table.
table AatSubtable {
    /// The length of this subtable in bytes, including this header.
    #[compile(self.compute_length())]
    length: u32,
    /// Circumstances under which this table is used.
    coverage: u16,
    /// The tuple index (used for variations fonts). This value specifies which tuple this subtable covers.
    tuple_index: u16,
    /// Subtable specific data.
    #[count(..)]
    data: [u8],
}

/// A subtable in an OT `kern` table.
table OtSubtable {
    /// Kern subtable version number-- set to 0.
    #[compile(0)]
    version: u16,
    /// The length of this subtable in bytes, including this header.
    #[compile(self.compute_length())]
    length: u16,
    /// Circumstances under which this table is used.
    coverage: u16,
    /// Subtable specific data.
    #[count(..)]
    data: [u8],
}

/// The type 0 `kern` subtable.
table Subtable0 {
    /// The number of kerning pairs in this subtable.
    n_pairs: u16,
    /// The largest power of two less than or equal to the value of nPairs, multiplied by the size in bytes of an entry in the subtable.
    search_range: u16,
    /// This is calculated as log2 of the largest power of two less than or equal to the value of nPairs. This value indicates how many iterations of the search loop have to be made. For example, in a list of eight items, there would be three iterations of the loop.
    entry_selector: u16,
    /// The value of nPairs minus the largest power of two less than or equal to nPairs. This is multiplied by the size in bytes of an entry in the table.
    range_shift: u16,
    /// Kerning records.
    #[count($n_pairs)]
    pairs: [Subtable0Pair],
}

/// Class table for the type 2 `kern` subtable.
table Subtable2ClassTable {
    /// First glyph in class range.
    first_glyph: GlyphId16,
    /// Number of glyph in class range.
    n_glyphs: u16,
    /// The offsets array for all of the glyphs in the range.
    #[count($n_glyphs)]
    offsets: [u16],
}
