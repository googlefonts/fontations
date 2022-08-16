#![parse_module(read_fonts::tables::cmap)]

/// [cmap](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#overview)
table Cmap {
    /// Table version number (0).
    version: BigEndian<u16>,
    /// Number of encoding tables that follow.
    #[compile(array_len($encoding_records))]
    num_tables: BigEndian<u16>,
    #[count($num_tables)]
    encoding_records: [EncodingRecord],
}

/// [Encoding Record](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#encoding-records-and-encodings)
record EncodingRecord {
    /// Platform ID.
    platform_id: BigEndian<u16>,
    /// Platform-specific encoding ID.
    encoding_id: BigEndian<u16>,
    /// Byte offset from beginning of table to the subtable for this
    /// encoding.
    subtable_offset: BigEndian<Offset32<CmapSubtable>>,
}

/// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#platform-ids>
enum u16 PlatformId {
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
    format: BigEndian<u16>,
    /// This is the length in bytes of the subtable.
    #[compile(256 + 6)]
    length: BigEndian<u16>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u16>,
    /// An array that maps character codes to glyph index values.
    #[count(256)]
    glyph_id_array: [BigEndian<u8>],
}

    /// [cmap Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-2-high-byte-mapping-through-table): High-byte mapping through table
table Cmap2 {
    /// Format number is set to 2.
    #[format = 2]
    format: BigEndian<u16>,
    /// This is the length in bytes of the subtable.
    #[compile(panic!("not implemented"))]
    length: BigEndian<u16>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u16>,
    /// Array that maps high bytes to subHeaders: value is subHeader
    /// index × 8.
    #[count(256)]
    sub_header_keys: [BigEndian<u16>],

    //FIXME: these two fields will require some custom handling
    ///// Variable-length array of SubHeader records.
    //#[count( )]
    //sub_headers: [SubHeader],
    ///// Variable-length array containing subarrays used for mapping the
    ///// low byte of 2-byte characters.
    //#[count( )]
    //glyph_id_array: [BigEndian<u16>],
}


/// Part of [Cmap2]
record SubHeader {
    /// First valid low byte for this SubHeader.
    first_code: BigEndian<u16>,
    /// Number of valid low bytes for this SubHeader.
    entry_count: BigEndian<u16>,
    /// See text below.
    id_delta: BigEndian<i16>,
    /// See text below.
    id_range_offset: BigEndian<u16>,
}

/// [cmap Format 4](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values): Segment mapping to delta values
table Cmap4 {
    /// Format number is set to 4.
    #[format = 4]
    format: BigEndian<u16>,
    /// This is the length in bytes of the subtable.
    length: BigEndian<u16>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u16>,
    /// 2 × segCount.
    seg_count_x2: BigEndian<u16>,
    /// Maximum power of 2 less than or equal to segCount, times 2
    /// ((2**floor(log2(segCount))) * 2, where “**” is an
    /// exponentiation operator)
    search_range: BigEndian<u16>,
    /// Log2 of the maximum power of 2 less than or equal to numTables
    /// (log2(searchRange/2), which is equal to floor(log2(segCount)))
    entry_selector: BigEndian<u16>,
    /// segCount times 2, minus searchRange ((segCount * 2) -
    /// searchRange)
    range_shift: BigEndian<u16>,
    /// End characterCode for each segment, last=0xFFFF.
    #[count($seg_count_x2 as usize / 2)]
    end_code: [BigEndian<u16>],
    /// Set to 0.
    #[skip_getter]
    reserved_pad: BigEndian<u16>,
    /// Start character code for each segment.
    #[count($seg_count_x2 as usize / 2)]
    start_code: [BigEndian<u16>],
    /// Delta for all character codes in segment.
    #[count($seg_count_x2 as usize / 2)]
    id_delta: [BigEndian<i16>],
    /// Offsets into glyphIdArray or 0
    #[count($seg_count_x2 as usize / 2)]
    id_range_offsets: [BigEndian<u16>],
    /// Glyph index array (arbitrary length)
    #[count(..)]
    glyph_id_array: [BigEndian<u16>],
}

/// [cmap Format 6](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-6-trimmed-table-mapping): Trimmed table mapping
table Cmap6 {
    /// Format number is set to 6.
    #[format = 6]
    format: BigEndian<u16>,
    /// This is the length in bytes of the subtable.
    length: BigEndian<u16>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u16>,
    /// First character code of subrange.
    first_code: BigEndian<u16>,
    /// Number of character codes in subrange.
    entry_count: BigEndian<u16>,
    /// Array of glyph index values for character codes in the range.
    #[count($entry_count)]
    glyph_id_array: [BigEndian<u16>],
}

/// [cmap Format 8](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-8-mixed-16-bit-and-32-bit-coverage): mixed 16-bit and 32-bit coverage
table Cmap8 {
    /// Subtable format; set to 8.
    #[format = 8]
    format: BigEndian<u16>,
    /// Reserved; set to 0
    #[skip_getter]
    reserved: BigEndian<u16>,
    /// Byte length of this subtable (including the header)
    length: BigEndian<u32>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u32>,
    /// Tightly packed array of bits (8K bytes total) indicating
    /// whether the particular 16-bit (index) value is the start of a
    /// 32-bit character code
    #[count(8192)]
    is32: [BigEndian<u8>],
    /// Number of groupings which follow
    num_groups: BigEndian<u32>,
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
    start_char_code: BigEndian<u32>,
    /// Last character code in this group; same condition as listed
    /// above for the startCharCode
    end_char_code: BigEndian<u32>,
    /// Glyph index corresponding to the starting character code
    start_glyph_id: BigEndian<u32>,
}

    /// [cmap Format 10](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-10-trimmed-array): Tr
table Cmap10 {
    /// Subtable format; set to 10.
    #[format = 10]
    format: BigEndian<u16>,
    /// Reserved; set to 0
    #[skip_getter]
    reserved: BigEndian<u16>,
    /// Byte length of this subtable (including the header)
    length: BigEndian<u32>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u32>,
    /// First character code covered
    start_char_code: BigEndian<u32>,
    /// Number of character codes covered
    num_chars: BigEndian<u32>,
    /// Array of glyph indices for the character codes covered
    #[count(..)]
    glyph_id_array: [BigEndian<u16>],
}

/// [cmap Format 12](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage): Segmented coverage
table Cmap12 {
    /// Subtable format; set to 12.
    #[format = 12]
    format: BigEndian<u16>,
    /// Reserved; set to 0
    #[skip_getter]
    reserved: BigEndian<u16>,
    /// Byte length of this subtable (including the header)
    length: BigEndian<u32>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u32>,
    /// Number of groupings which follow
    num_groups: BigEndian<u32>,
    /// Array of SequentialMapGroup records.
    #[count($num_groups)]
    groups: [SequentialMapGroup],
}

/// [cmap Format 13](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-13-many-to-one-range-mappings): Many-to-one range mappings
table Cmap13 {
    /// Subtable format; set to 13.
    #[format = 13]
    format: BigEndian<u16>,
    /// Reserved; set to 0
    #[skip_getter]
    reserved: BigEndian<u16>,
    /// Byte length of this subtable (including the header)
    length: BigEndian<u32>,
    /// For requirements on use of the language field, see “Use of
    /// the language field in 'cmap' subtables” in this document.
    language: BigEndian<u32>,
    /// Number of groupings which follow
    num_groups: BigEndian<u32>,
    /// Array of ConstantMapGroup records.
    #[count($num_groups)]
    groups: [ConstantMapGroup],
}

/// Part of [Cmap13]
record ConstantMapGroup {
    /// First character code in this group
    start_char_code: BigEndian<u32>,
    /// Last character code in this group
    end_char_code: BigEndian<u32>,
    /// Glyph index to be used for all the characters in the group’s
    /// range.
    glyph_id: BigEndian<u32>,
}

/// [cmap Format 14](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-14-unicode-variation-sequences): Unicode Variation Sequences
table Cmap14 {
    /// Subtable format. Set to 14.
    #[format = 14]
    format: BigEndian<u16>,
    /// Byte length of this subtable (including this header)
    length: BigEndian<u32>,
    /// Number of variation Selector Records
    num_var_selector_records: BigEndian<u32>,
    /// Array of VariationSelector records.
    #[count($num_var_selector_records)]
    var_selector: [VariationSelector],
}

/// Part of [Cmap14]
record VariationSelector {
    /// Variation selector
    var_selector: BigEndian<Uint24>,
    /// Offset from the start of the format 14 subtable to Default UVS
    /// Table. May be 0.
    default_uvs_offset: BigEndian<Offset32>,
    /// Offset from the start of the format 14 subtable to Non-Default
    /// UVS Table. May be 0.
    non_default_uvs_offset: BigEndian<Offset32>,
}

/// [Default UVS table](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#default-uvs-table)
table DefaultUvs {
    /// Number of Unicode character ranges.
    num_unicode_value_ranges: BigEndian<u32>,
    /// Array of UnicodeRange records.
    #[count($num_unicode_value_ranges)]
    ranges: [UnicodeRange],
}

/// Part of [Cmap14]
record UVSMapping {
    /// Base Unicode value of the UVS
    unicode_value: BigEndian<Uint24>,
    /// Glyph ID of the UVS
    glyph_id: BigEndian<u16>,
}

/// Part of [Cmap14]
record UnicodeRange {
    /// First value in this range
    start_unicode_value: BigEndian<Uint24>,
    /// Number of additional values in this range
    additional_count: BigEndian<u8>,
}
