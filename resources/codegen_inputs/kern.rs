#![parse_module(read_fonts::tables::kern)]

/// The [kern (Kerning)](https://docs.microsoft.com/en-us/typography/opentype/spec/kern) table
#[tag = "kern"]
table Kern {
    /// Table version number — set to 0.
    #[compile(0)]
    version: u16,
    /// Number of subtables in the kerning table
    #[compile(array_len($subtables))]
    num_tables: u16,
    #[count($num_tables)]
    #[traverse_with(skip)]
    subtables: VarLenArray<Kern0>,
}

///// The different kern subtable formats.
//format u16 KernSubtable {
    //Format0(Kern0),
    //// Nope.
    //// Format2(Kern2),
//}

/// The `macStyle` field for the head table.
flags u16 KernCoverage {
    /// Bit 0: 1 if horizontal, 0 if vertical
    HORIZONTAL = 0x0001,
    /// Bit 1: 1 if table contains minimum values, 0 if kern values
    MINIMUM = 0x0002,
    /// Bit 2: If set to 1, kerning is perpendicular to the flow of the text.
    ///
    /// If the text is normally written horizontally, kerning will be done in
    /// the up and down directions. If kerning values are positive, the text
    /// will be kerned upwards; if they are negative, the text will be kerned
    /// downwards.
    ///
    /// If the text is normally written vertically, kerning will be done in the
    /// left and right directions. If kerning values are positive, the text
    /// will be kerned to the right; if they are negative, the text will be
    /// kerned to the left.
    ///
    /// The value 0x8000 in the kerning data resets the cross-stream kerning
    /// back to 0.
    CROSS_STREAM = 0x0004,
    /// Bit 3: If this bit is set to 1 the value in this table should replace
    /// the value currently being accumulated.
    OVERRIDE = 0x0008,
    /// Bit 4: Shadow (if set to 1)
    SHADOW = 0x0010,
    /// Bit 5: Condensed (if set to 1)
    CONDENSED = 0x0020,
    /// Bit 6: Extended (if set to 1)
    EXTENDED = 0x0040,
    // Bits 7-15: Reserved (set to 0)
}

/// [kern Format 0](https://docs.microsoft.com/en-us/typography/opentype/spec/kern#format-0)
table Kern0 {
    /// Format number is set to 0.
    #[format = 0]
    format: u16,
    /// The length of the subtable, in bytes (including this header).
    #[compile(self.compute_length())]
    length: u16,
    /// What type of information is contained in this table.
    coverage: KernCoverage,
    /// This gives the number of kerning pairs in the table.
    #[compile(array_len($kerning_pairs))]
    num_pairs: u16,
    /// The largest power of two less than or equal to the value of num_pairs, multiplied by the
    /// size in bytes of an entry in the table.
    search_range: u16,
    /// This is calculated as log2 of the largest power of two less than or equal to the value of num_pairs.
    /// This value indicates how many iterations of the search loop will have to be made.
    /// (For example, in a list of eight items, there would have to be three iterations of the loop).
    entry_selector: u16,
    /// The value of num_pairs minus the largest power of two less than or equal to num_pairs,
    /// and then multiplied by the size in bytes of an entry in the table.
    range_shift: u16,
    /// Kern pairs
    #[count($num_pairs)]
    kerning_pairs: [KernPair],
}

record KernPair {
    /// The glyph index for the left-hand glyph in the kerning pair.
    left: u16,
    /// The glyph index for the right-hand glyph in the kerning pair.
    right: u16,
    /// The kerning value for the above pair, in font design units.
    /// If this value is greater than zero, the characters will be moved apart.
    /// If this value is less than zero, the character will be moved closer together.
    value: FWord,
}
