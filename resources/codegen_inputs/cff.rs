#![parse_module(read_fonts::tables::cff)]

/// [Compact Font Format](https://learn.microsoft.com/en-us/typography/opentype/spec/cff) table
#[tag = "CFF "]
table Cff {
    /// Format major version (starting at 1).
    #[compile(1)]
    major: u8,
    /// Format minor version (starting at 0).
    #[compile(0)]
    minor: u8,
    /// Header size (bytes).
    hdr_size: u8,
    /// Absolute offset size.
    off_size: u8,
    /// Padding bytes before the start of the Name INDEX.
    #[count(subtract($hdr_size, 4))]
    _padding: [u8],
    /// Remaining table data.
    #[count(..)]
    trailing_data: [u8],
}
