#![parse_module(read_fonts::tables::cmap)]

/// [cmap](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#overview)
#[tag = "cmap"]
table Cmap {
    /// Table version number (0).
    #[compile(0)]
    version: u16,
    /// Number of encoding tables that follow.
    #[compile(array_len($encoding_records))]
    num_tables: u16,
    #[count($num_tables)]
    encoding_records: [EncodingRecord],
}

/// [Encoding Record](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#encoding-records-and-encodings)
record EncodingRecord {
    /// Platform ID.
    platform_id: PlatformId,
    /// Platform-specific encoding ID.
    encoding_id: u16,
    /// Byte offset from beginning of the [`Cmap`] table to the subtable for this
    /// encoding.
    subtable_offset: Offset32<CmapSubtable>,
}

/// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#platform-ids>
enum u16 PlatformId {
    #[default]
    Unicode = 0,
    Macintosh = 1,
    ISO  = 2,
    Windows = 3,
    Custom = 4,
}

/// The different cmap subtable formats.
format u16 CmapSubtable {
    Format0(Cmap0),
    Format2(Cmap2),
    Format4(Cmap4),
    Format6(Cmap6),
    Format8(Cmap8),
    Format10(Cmap10),
    Format12(Cmap12),
    Format13(Cmap13),
    Format14(Cmap14),
}

/// [cmap Format 0](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-0-byte-encoding-table): Byte encoding table
table Cmap0 {
    /// Format number is set to 0.
    #[format = 0]
    format: u16,
    /// This is the length in bytes of the subtable.
    #[compile(256 + 6)]
    length: u16,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u16,
    /// An array that maps character codes to glyph index values.
    #[count(256)]
    glyph_id_array: [u8],
}

    /// [cmap Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-2-high-byte-mapping-through-table): High-byte mapping through table
table Cmap2 {
    /// Format number is set to 2.
    #[format = 2]
    format: u16,
    /// This is the length in bytes of the subtable.
    length: u16,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u16,
    /// Array that maps high bytes to subHeaders: value is subHeader
    /// index × 8.
    #[count(256)]
    sub_header_keys: [u16],

    //FIXME: these two fields will require some custom handling
    ///// Variable-length array of SubHeader records.
    //#[count( )]
    //sub_headers: [SubHeader],
    ///// Variable-length array containing subarrays used for mapping the
    ///// low byte of 2-byte characters.
    //#[count( )]
    //glyph_id_array: [u16],
}


/// Part of [Cmap2]
record SubHeader {
    /// First valid low byte for this SubHeader.
    first_code: u16,
    /// Number of valid low bytes for this SubHeader.
    entry_count: u16,
    /// See text below.
    id_delta: i16,
    /// See text below.
    id_range_offset: u16,
}

/// [cmap Format 4](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values): Segment mapping to delta values
table Cmap4 {
    /// Format number is set to 4.
    #[format = 4]
    format: u16,
    /// This is the length in bytes of the subtable.
    #[compile(self.compute_length())]
    length: u16,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u16,
    /// 2 × segCount.
    #[compile(2 * array_len($end_code))]
    seg_count_x2: u16,
    /// Maximum power of 2 less than or equal to segCount, times 2
    /// ((2**floor(log2(segCount))) * 2, where “**” is an
    /// exponentiation operator)
    #[compile(self.compute_search_range())]
    search_range: u16,
    /// Log2 of the maximum power of 2 less than or equal to numTables
    /// (log2(searchRange/2), which is equal to floor(log2(segCount)))
    #[compile(self.compute_entry_selector())]
    entry_selector: u16,
    /// segCount times 2, minus searchRange ((segCount * 2) -
    /// searchRange)
    #[compile(self.compute_range_shift())]
    range_shift: u16,
    /// End characterCode for each segment, last=0xFFFF.
    #[count(half($seg_count_x2))]
    end_code: [u16],
    /// Set to 0.
    #[skip_getter]
    #[compile(0)]
    reserved_pad: u16,
    /// Start character code for each segment.
    #[count(half($seg_count_x2))]
    start_code: [u16],
    /// Delta for all character codes in segment.
    #[count(half($seg_count_x2))]
    id_delta: [i16],
    /// Offsets into glyphIdArray or 0
    #[count(half($seg_count_x2))]
    id_range_offsets: [u16],
    /// Glyph index array (arbitrary length)
    #[count(..)]
    glyph_id_array: [u16],
}

/// [cmap Format 6](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-6-trimmed-table-mapping): Trimmed table mapping
table Cmap6 {
    /// Format number is set to 6.
    #[format = 6]
    format: u16,
    /// This is the length in bytes of the subtable.
    length: u16,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u16,
    /// First character code of subrange.
    first_code: u16,
    /// Number of character codes in subrange.
    entry_count: u16,
    /// Array of glyph index values for character codes in the range.
    #[count($entry_count)]
    glyph_id_array: [u16],
}

/// [cmap Format 8](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-8-mixed-16-bit-and-32-bit-coverage): mixed 16-bit and 32-bit coverage
table Cmap8 {
    /// Subtable format; set to 8.
    #[format = 8]
    format: u16,
    /// Reserved; set to 0
    #[skip_getter]
    #[compile(0)]
    reserved: u16,
    /// Byte length of this subtable (including the header)
    length: u32,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u32,
    /// Tightly packed array of bits (8K bytes total) indicating
    /// whether the particular 16-bit (index) value is the start of a
    /// 32-bit character code
    #[count(8192)]
    is32: [u8],
    /// Number of groupings which follow
    num_groups: u32,
    /// Array of SequentialMapGroup records.
    #[count($num_groups)]
    groups: [SequentialMapGroup],
}

/// Used in [Cmap8] and [Cmap12]
record SequentialMapGroup {
    /// First character code in this group; note that if this group is
    /// for one or more 16-bit character codes (which is determined
    /// from the is32 array), this 32-bit value will have the high
    /// 16-bits set to zero
    start_char_code: u32,
    /// Last character code in this group; same condition as listed
    /// above for the startCharCode
    end_char_code: u32,
    /// Glyph index corresponding to the starting character code
    start_glyph_id: u32,
}

    /// [cmap Format 10](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-10-trimmed-array): Tr
table Cmap10 {
    /// Subtable format; set to 10.
    #[format = 10]
    format: u16,
    /// Reserved; set to 0
    #[skip_getter]
    #[compile(0)]
    reserved: u16,
    /// Byte length of this subtable (including the header)
    length: u32,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u32,
    /// First character code covered
    start_char_code: u32,
    /// Number of character codes covered
    num_chars: u32,
    /// Array of glyph indices for the character codes covered
    #[count(..)]
    glyph_id_array: [u16],
}

/// [cmap Format 12](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage): Segmented coverage
table Cmap12 {
    /// Subtable format; set to 12.
    #[format = 12]
    format: u16,
    /// Reserved; set to 0
    #[skip_getter]
    #[compile(0)]
    reserved: u16,
    /// Byte length of this subtable (including the header)
    #[compile(self.compute_length())]
    length: u32,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u32,
    /// Number of groupings which follow
    #[compile(array_len($groups))]
    num_groups: u32,
    /// Array of SequentialMapGroup records.
    #[count($num_groups)]
    groups: [SequentialMapGroup],
}

/// [cmap Format 13](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-13-many-to-one-range-mappings): Many-to-one range mappings
table Cmap13 {
    /// Subtable format; set to 13.
    #[format = 13]
    format: u16,
    /// Reserved; set to 0
    #[skip_getter]
    #[compile(0)]
    reserved: u16,
    /// Byte length of this subtable (including the header)
    length: u32,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: u32,
    /// Number of groupings which follow
    num_groups: u32,
    /// Array of ConstantMapGroup records.
    #[count($num_groups)]
    groups: [ConstantMapGroup],
}

/// Part of [Cmap13]
record ConstantMapGroup {
    /// First character code in this group
    start_char_code: u32,
    /// Last character code in this group
    end_char_code: u32,
    /// Glyph index to be used for all the characters in the group’s
    /// range.
    glyph_id: u32,
}

/// [cmap Format 14](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-14-unicode-variation-sequences): Unicode Variation Sequences
table Cmap14 {
    /// Subtable format. Set to 14.
    #[format = 14]
    format: u16,
    /// Byte length of this subtable (including this header)
    length: u32,
    /// Number of variation Selector Records
    num_var_selector_records: u32,
    /// Array of VariationSelector records.
    #[count($num_var_selector_records)]
    var_selector: [VariationSelector],
}

/// Part of [Cmap14]
record VariationSelector {
    /// Variation selector
    var_selector: Uint24,
    /// Offset from the start of the [`Cmap14`] subtable to Default UVS
    /// Table. May be NULL.
    #[nullable]
    default_uvs_offset: Offset32<DefaultUvs>,
    /// Offset from the start of the [`Cmap14`] subtable to Non-Default
    /// UVS Table. May be NULL.
    #[nullable]
    non_default_uvs_offset: Offset32<NonDefaultUvs>,
}

/// [Default UVS table](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#default-uvs-table)
table DefaultUvs {
    /// Number of Unicode character ranges.
    num_unicode_value_ranges: u32,
    /// Array of UnicodeRange records.
    #[count($num_unicode_value_ranges)]
    ranges: [UnicodeRange],
}

/// [Non-Default UVS table](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#non-default-uvs-table)
table NonDefaultUvs {
    num_uvs_mappings: u32,
    #[count($num_uvs_mappings)]
    uvs_mapping: [UvsMapping]

}

/// Part of [Cmap14]
record UvsMapping {
    /// Base Unicode value of the UVS
    unicode_value: Uint24,
    /// Glyph ID of the UVS
    glyph_id: u16,
}

/// Part of [Cmap14]
record UnicodeRange {
    /// First value in this range
    start_unicode_value: Uint24,
    /// Number of additional values in this range
    additional_count: u8,
}
