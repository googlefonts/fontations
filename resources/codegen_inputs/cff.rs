#![parse_module(read_fonts::ps::cff::v1)]

/// [Compact Font Format](https://learn.microsoft.com/en-us/typography/opentype/spec/cff) table header
table CffHeader {
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

/// An array of variable-sized objects in a `CFF` table.
table Index {
    /// Number of objects stored in INDEX.
    count: u16,
    /// Object array element size.
    off_size: u8,
    /// Bytes containing `count + 1` offsets each of `off_size`.
    #[count(add_multiply($count, 1, $off_size))]
    offsets: [u8],
    /// Array containing the object data.
    #[count(..)]
    data: [u8],
}

/// Associates a glyph identifier with a Font DICT.
format u8 FdSelect {
    Format0(FdSelectFormat0),
    Format3(FdSelectFormat3),
    Format4(FdSelectFormat4),
}

/// FdSelect format 0.
table FdSelectFormat0 {
    /// Format = 0.
    #[format = 0]
    format: u8,
    /// FD selector array (one entry for each glyph).
    #[count(..)]
    fds: [u8],
}

/// FdSelect format 3.
table FdSelectFormat3 {
    /// Format = 3.
    #[format = 3]
    format: u8,
    /// Number of ranges.
    #[compile(array_len($ranges))]
    n_ranges: u16,
    /// Range3 array.
    #[count($n_ranges)]
    ranges: [FdSelectRange3],
    /// Sentinel GID. Set equal to the number of glyphs in the font.
    sentinel: u16,
}

/// Range struct for FdSelect format 3.
record FdSelectRange3 {
    /// First glyph index in range.
    first: u16,
    /// FD index for all glyphs in range.
    fd: u8,
}

/// FdSelect format 4.
table FdSelectFormat4 {
    /// Format = 4.
    #[format = 4]
    format: u8,
    /// Number of ranges.
    #[compile(array_len($ranges))]
    n_ranges: u32,
    /// Range4 array.
    #[count($n_ranges)]
    ranges: [FdSelectRange4],
    /// Sentinel GID. Set equal to the number of glyphs in the font.
    sentinel: u32,
}

/// Range struct for FdSelect format 4.
record FdSelectRange4 {
    /// First glyph index in range.
    first: u32,
    /// FD index for all glyphs in range.
    fd: u16,
}

/// Charset with custom glyph id to string id mappings.
format u8 CustomCharset {
    Format0(CharsetFormat0),
    Format1(CharsetFormat1),
    Format2(CharsetFormat2),
}

/// Charset format 0.
table CharsetFormat0 {
    /// Format; =0
    #[format = 0]
    format: u8,
    /// Glyph name array.
    #[count(..)]
    glyph: [u16],
}

/// Charset format 1.
table CharsetFormat1 {
    /// Format; =1
    #[format = 1]
    format: u8,
    /// Range1 array.
    #[count(..)]
    ranges: [CharsetRange1],
}

/// Range struct for Charset format 1.
record CharsetRange1 {
    /// First glyph in range.
    first: u16,
    /// Glyphs left in range (excluding first).
    n_left: u8,
}

/// Charset format 2.
table CharsetFormat2 {
    /// Format; =2
    #[format = 2]
    format: u8,
    /// Range2 array.
    #[count(..)]
    ranges: [CharsetRange2],
}

/// Range struct for Charset format 2.
record CharsetRange2 {
    /// First glyph in range.
    first: u16,
    /// Glyphs left in range (excluding first).
    n_left: u16,
}

/// Range struct for Encoding format 1.
record EncodingRange1 {
    /// First code in range.
    first: u8,
    /// Codes left in range (excluding first).
    n_left: u8,
}

/// Supplemental encoding record.
record EncodingSupplement {
    /// Encoding.
    code: u8,
    /// Name.
    glyph: u16
}
