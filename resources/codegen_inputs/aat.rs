#![parse_module(read_fonts::tables::aat)]

/// Lookup tables provide a way of looking up information about a glyph index.
/// The different cmap subtable formats.
format u16 Lookup {
    Format0(Lookup0),
    Format2(Lookup2),
    Format4(Lookup4),
    Format6(Lookup6),
    Format8(Lookup8),
    Format10(Lookup10),
}

/// Simple array format. The lookup data is an array of lookup values, indexed
/// by glyph index.
table Lookup0 {
    /// Format number is set to 0.
    #[format = 0]
    format: u16,
    /// Values, indexed by glyph index.
    #[count(..)]
    values_data: [u8],
}

/// Segment single format. Each non-overlapping segment has a single lookup
/// value that applies to all glyphs in the segment. A segment is defined as
/// a contiguous range of glyph indexes.
table Lookup2 {
    /// Format number is set to 2.
    #[format = 2]
    format: u16,
    /// Size of a lookup unit for this search in bytes.
    unit_size: u16,
    /// Number of units of the preceding size to be searched.
    n_units: u16,
    /// The value of unitSize times the largest power of 2 that is less than or equal to the value of nUnits. 
    search_range: u16,
    /// The log base 2 of the largest power of 2 less than or equal to the value of nUnits.
    entry_selector: u16,
    /// The value of unitSize times the difference of the value of nUnits minus the largest power of 2 less than or equal to the value of nUnits.
    range_shift: u16,
    /// Segments.
    #[count(add_multiply($unit_size, 0, $n_units))]
    segments_data: [u8],
}

/// Segment array format. A segment mapping is performed (as with Format 2),
/// but instead of a single lookup value for all the glyphs in the segment,
/// each glyph in the segment gets its own separate lookup value.
table Lookup4 {
    /// Format number is set to 4.
    #[format = 4]
    format: u16,
    /// Size of a lookup unit for this search in bytes.
    unit_size: u16,
    /// Number of units of the preceding size to be searched.
    n_units: u16,
    /// The value of unitSize times the largest power of 2 that is less than or equal to the value of nUnits. 
    search_range: u16,
    /// The log base 2 of the largest power of 2 less than or equal to the value of nUnits.
    entry_selector: u16,
    /// The value of unitSize times the difference of the value of nUnits minus the largest power of 2 less than or equal to the value of nUnits.
    range_shift: u16,
    /// Segments.
    #[count($n_units)]
    segments: [LookupSegment4],
}

/// Lookup segment for format 4.
record LookupSegment4 {
    /// Last glyph index in this segment.
    last_glyph: u16,
    /// First glyph index in this segment.
    first_glyph: u16,
    /// A 16-bit offset from the start of the table to the data.
    value_offset: u16,
}

/// Single table format. The lookup data is a sorted list of
/// <glyph index,lookup value> pairs.
table Lookup6 {
    /// Format number is set to 6.
    #[format = 6]
    format: u16,
    /// Size of a lookup unit for this search in bytes.
    unit_size: u16,
    /// Number of units of the preceding size to be searched.
    n_units: u16,
    /// The value of unitSize times the largest power of 2 that is less than or equal to the value of nUnits. 
    search_range: u16,
    /// The log base 2 of the largest power of 2 less than or equal to the value of nUnits.
    entry_selector: u16,
    /// The value of unitSize times the difference of the value of nUnits minus the largest power of 2 less than or equal to the value of nUnits.
    range_shift: u16,
    /// Values, indexed by glyph index.
    #[count(add_multiply($unit_size, 0, $n_units))]
    entries_data: [u8],
}

/// Trimmed array format. The lookup data is a simple trimmed array
/// indexed by glyph index.
table Lookup8 {
    /// Format number is set to 8.
    #[format = 8]
    format: u16,
    /// First glyph index included in the trimmed array.
    first_glyph: u16,
    /// Total number of glyphs (equivalent to the last glyph minus the value
    /// of firstGlyph plus 1).
    glyph_count: u16,
    /// The lookup values (indexed by the glyph index minus the value of
    /// firstGlyph). Entries in the value array must be two bytes.
    #[count($glyph_count)]
    value_array: [u16],
}

/// Trimmed array format. The lookup data is a simple trimmed array
/// indexed by glyph index.
table Lookup10 {
    /// Format number is set to 10.
    #[format = 10]
    format: u16,
    /// Size of a lookup unit for this lookup table in bytes. Allowed values
    /// are 1, 2, 4, and 8.
    unit_size: u16,
    /// First glyph index included in the trimmed array.
    first_glyph: u16,
    /// Total number of glyphs (equivalent to the last glyph minus the value
    /// of firstGlyph plus 1).
    glyph_count: u16,
    /// The lookup values (indexed by the glyph index minus the value of
    /// firstGlyph).
    #[count(add_multiply($glyph_count, 0, $unit_size))]
    values_data: [u8],
}

/// Header for a state table.
table StateHeader {
    /// Size of a state, in bytes. The size is limited to 8 bits, although the
    /// field is 16 bits for alignment.
    state_size: u16,
    /// Byte offset from the beginning of the state table to the class subtable.
    class_table_offset: Offset16<ClassSubtable>,
    /// Byte offset from the beginning of the state table to the state array.
    state_array_offset: Offset16<RawBytes>,
    /// Byte offset from the beginning of the state table to the entry subtable.
    entry_table_offset: Offset16<RawBytes>,
}

/// Maps the glyph indexes of your font into classes.
table ClassSubtable {
    /// Glyph index of the first glyph in the class table.
    first_glyph: u16,
    /// Number of glyphs in class table.
    n_glyphs: u16,
    /// The class codes (indexed by glyph index minus firstGlyph). Class codes
    /// range from 0 to the value of stateSize minus 1.
    #[count($n_glyphs)]
    class_array: [u8],
}

/// Used for the `state_array` and `entry_table` fields in [`StateHeader`].
table RawBytes {
    #[count(..)]
    data: [u8]
}

/// Header for an extended state table.
table StxHeader {
    /// Number of classes, which is the number of 16-bit entry indices in a single line in the state array.
    n_classes: u32,
    /// Byte offset from the beginning of the state table to the class subtable.
    class_table_offset: Offset32<LookupU16>,
    /// Byte offset from the beginning of the state table to the state array.
    state_array_offset: Offset32<RawWords>,
    /// Byte offset from the beginning of the state table to the entry subtable.
    entry_table_offset: Offset32<RawBytes>,
}

/// Used for the `state_array` in [`StxHeader`].
table RawWords {
    #[count(..)]
    data: [u16]
}