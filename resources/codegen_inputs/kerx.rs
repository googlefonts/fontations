#![parse_module(read_fonts::tables::morx)]

/// The [kerx (Extended Kerning)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6morx.html) table.
#[tag = "kerx"]
table Kerx {
    /// The version number of the extended kerning table (currently 2, 3, or 4)
    version: u16,
    /// Unused; set to zero.
    #[skip_getter]
    #[compile(0)]
    padding: u16,
    /// The number of subtables included in the extended kerning table.
    #[compile(array_len($chains))]
    n_tables: u32,
    #[count($n_tables)]
    subtables: VarLenArray<Subtable<'a>>,
}

/// A subtable in a `kerx` table.
table Subtable {
    /// The length of this subtable in bytes, including this header.
    #[compile(self.compute_length())]
    length: u32,
    /// Circumstances under which this table is used.
    coverage: u32,
    /// The tuple count. This value is only used with variation fonts and should be 0 for all other fonts. The subtable's tupleCount will be ignored if the 'kerx' table version is less than 4.
    tuple_count: u32,
    /// Subtable specific data.
    #[count(..)]
    data: [u8],
}

/// The type 0 `kerx` subtable.
table Subtable0 {
    /// The number of kerning pairs in this subtable.
    n_pairs: u32,
    /// The largest power of two less than or equal to the value of nPairs, multiplied by the size in bytes of an entry in the subtable.
    search_range: u32,
    /// This is calculated as log2 of the largest power of two less than or equal to the value of nPairs. This value indicates how many iterations of the search loop have to be made. For example, in a list of eight items, there would be three iterations of the loop.
    entry_selector: u32,
    /// The value of nPairs minus the largest power of two less than or equal to nPairs. This is multiplied by the size in bytes of an entry in the table.
    range_shift: u32,
    /// Kerning records.
    #[count($n_pairs)]
    pairs: [Subtable0Pair],
}

/// The type 0 `kerx` subtable kerning record.
record Subtable0Pair {
    /// The glyph index for the lefthand glyph in the kerning pair.
    left: GlyphId16,
    /// The glyph index for the righthand glyph in the kerning pair.
    right: GlyphId16,
    /// Kerning value.
    value: i16,
}
