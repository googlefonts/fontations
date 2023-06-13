#![parse_module(read_fonts::tables::cff2)]

/// [Compact Font Format (CFF) version 2](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2) table header
table Cff2Header {
    /// Format major version (set to 2).
    #[compile(2)]
    major_version: u8,
    /// Format minor version (set to 0).
    #[compile(0)]
    minor_version: u8,
    /// Header size (bytes).
    header_size: u8,
    /// Length of Top DICT structure in bytes.
    top_dict_length: u16,
    /// Padding bytes before the start of the Top DICT.
    #[count(subtract($header_size, 5))]
    _padding: [u8],
    /// Data containing the Top DICT.
    #[count($top_dict_length)]
    top_dict_data: [u8],
    /// Remaining table data.
    #[count(..)]
    trailing_data: [u8],
}
