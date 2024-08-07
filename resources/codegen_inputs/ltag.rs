#![parse_module(read_fonts::tables::ltag)]

/// The [language tag](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6ltag.html) table.
#[tag = "ltag"]
table Ltag {
    /// Table version; currently 1.
    version: u32,
    /// Table flags; currently none defined.
    flags: u32,
    /// Number of language tags which follow.
    num_tags: u32,
    /// Range of each tag's string.
    #[count($num_tags)]
    tag_ranges: [FTStringRange],
}

/// Offset and length of string in `ltag` table.
record FTStringRange {
    /// Offset from the start of the table to the beginning of the string.
    offset: u16,
    /// String length (in bytes).
    length: u16,
}
