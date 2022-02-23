//! The [cmap](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap) table
use font_types::{BigEndian, Offset32, Uint24};

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#overview>
    #[offset_host]
    Cmap<'a> {
        /// Table version number (0).
        version: BigEndian<u16>,
        /// Number of encoding tables that follow.
        num_tables: BigEndian<u16>,
        #[count(num_tables)]
        encoding_records: [EncodingRecord],
    }

    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#encoding-records-and-encodings>
    EncodingRecord {
        /// Platform ID.
        platform_id: BigEndian<u16>,
        /// Platform-specific encoding ID.
        encoding_id: BigEndian<u16>,
        /// Byte offset from beginning of table to the subtable for this
        /// encoding.
        subtable_offset: BigEndian<Offset32>,
    }

    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#platform-ids>
    #[repr(u16)]
    enum PlatformId {
        Unicode = 0,
        Macintosh = 1,
        ISO  = 2,
        Windows = 3,
        Custom = 4,
    }

    /// The different cmap subtable formats.
    #[format(u16)]
    enum CmapSubtable<'a> {
        #[version(0)]
        Format0(Cmap0<'a>),
        #[version(2)]
        Format2(Cmap2<'a>),
        #[version(4)]
        Format4(Cmap4<'a>),
        #[version(6)]
        Format6(Cmap6<'a>),
        #[version(8)]
        Format8(Cmap8<'a>),
        #[version(10)]
        Format10(Cmap10<'a>),
        #[version(12)]
        Format12(Cmap12<'a>),
        #[version(13)]
        Format13(Cmap13<'a>),
        #[version(14)]
        Format14(Cmap14<'a>),
    }

}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-0-byte-encoding-table>
    Cmap0<'a> {
        /// Format number is set to 0.
        format: BigEndian<u16>,
        /// This is the length in bytes of the subtable.
        length: BigEndian<u16>,
        /// For requirements on use of the language field, see “Use of
        /// the language field in 'cmap' subtables” in this document.
        language: BigEndian<u16>,
        /// An array that maps character codes to glyph index values.
        #[count(256)]
        glyph_id_array: [BigEndian<u8>],
    }
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-2-high-byte-mapping-through-table>
    Cmap2<'a> {
        /// Format number is set to 2.
        format: BigEndian<u16>,
        /// This is the length in bytes of the subtable.
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

    SubHeader {
        /// First valid low byte for this SubHeader.
        first_code: BigEndian<u16>,
        /// Number of valid low bytes for this SubHeader.
        entry_count: BigEndian<u16>,
        /// See text below.
        id_delta: BigEndian<i16>,
        /// See text below.
        id_range_offset: BigEndian<u16>,
    }
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values>
    Cmap4<'a> {
        /// Format number is set to 4.
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
        #[count_with(div_by_two, seg_count_x2)]
        end_code: [BigEndian<u16>],
        /// Set to 0.
        #[hidden]
        reserved_pad: BigEndian<u16>,
        /// Start character code for each segment.
        #[count_with(div_by_two, seg_count_x2)]
        start_code: [BigEndian<u16>],
        /// Delta for all character codes in segment.
        #[count_with(div_by_two, seg_count_x2)]
        id_delta: [BigEndian<i16>],
        /// Offsets into glyphIdArray or 0
        #[count_with(div_by_two, seg_count_x2)]
        id_range_offsets: [BigEndian<u16>],
        /// Glyph index array (arbitrary length)
        #[count_all]
        glyph_id_array: [BigEndian<u16>],
    }
}

fn div_by_two(seg_count_x2: u16) -> usize {
    (seg_count_x2 / 2) as usize
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-6-trimmed-table-mapping>
    Cmap6<'a> {
        /// Format number is set to 6.
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
        #[count(entry_count)]
        glyph_id_array: [BigEndian<u16>],
    }
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-8-mixed-16-bit-and-32-bit-coverage>
    Cmap8<'a> {
        /// Subtable format; set to 8.
        format: BigEndian<u16>,
        /// Reserved; set to 0
        #[hidden]
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
        #[count(num_groups)]
        groups: [SequentialMapGroup],
    }

    SequentialMapGroup {
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
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-10-trimmed-array>
    Cmap10<'a> {
        /// Subtable format; set to 10.
        format: BigEndian<u16>,
        /// Reserved; set to 0
        #[hidden]
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
        #[count_all]
        glyph_id_array: [BigEndian<u16>],
    }
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage>
    Cmap12<'a> {
        /// Subtable format; set to 12.
        format: BigEndian<u16>,
        /// Reserved; set to 0
        #[hidden]
        reserved: BigEndian<u16>,
        /// Byte length of this subtable (including the header)
        length: BigEndian<u32>,
        /// For requirements on use of the language field, see “Use of
        /// the language field in 'cmap' subtables” in this document.
        language: BigEndian<u32>,
        /// Number of groupings which follow
        num_groups: BigEndian<u32>,
        /// Array of SequentialMapGroup records.
        #[count(num_groups)]
        groups: [SequentialMapGroup],
    }
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-13-many-to-one-range-mappings>
    Cmap13<'a> {
        /// Subtable format; set to 13.
        format: BigEndian<u16>,
        /// Reserved; set to 0
        #[hidden]
        reserved: BigEndian<u16>,
        /// Byte length of this subtable (including the header)
        length: BigEndian<u32>,
        /// For requirements on use of the language field, see “Use of
        /// the language field in 'cmap' subtables” in this document.
        language: BigEndian<u32>,
        /// Number of groupings which follow
        num_groups: BigEndian<u32>,
        /// Array of ConstantMapGroup records.
        #[count(num_groups)]
        groups: [ConstantMapGroup],
    }

    ConstantMapGroup {
        /// First character code in this group
        start_char_code: BigEndian<u32>,
        /// Last character code in this group
        end_char_code: BigEndian<u32>,
        /// Glyph index to be used for all the characters in the group’s
        /// range.
        glyph_id: BigEndian<u32>,
    }
}

font_types::tables! {
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#format-14-unicode-variation-sequences>
    #[offset_host]
    Cmap14<'a> {
        /// Subtable format. Set to 14.
        format: BigEndian<u16>,
        /// Byte length of this subtable (including this header)
        length: BigEndian<u32>,
        /// Number of variation Selector Records
        num_var_selector_records: BigEndian<u32>,
        /// Array of VariationSelector records.
        #[count(num_var_selector_records)]
        var_selector: [VariationSelector],
    }

    VariationSelector {
        /// Variation selector
        var_selector: BigEndian<Uint24>,
        /// Offset from the start of the format 14 subtable to Default UVS
        /// Table. May be 0.
        default_uvs_offset: BigEndian<Offset32>,
        /// Offset from the start of the format 14 subtable to Non-Default
        /// UVS Table. May be 0.
        non_default_uvs_offset: BigEndian<Offset32>,
    }

    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#default-uvs-table>
    DefaultUvs<'a> {
        /// Number of Unicode character ranges.
        num_unicode_value_ranges: BigEndian<u32>,
        /// Array of UnicodeRange records.
        #[count(num_unicode_value_ranges)]
        ranges: [UnicodeRange],
    }

    UVSMapping {
        /// Base Unicode value of the UVS
        unicode_value: BigEndian<Uint24>,
        /// Glyph ID of the UVS
        glyph_id: BigEndian<u16>,
    }

    UnicodeRange {
        /// First value in this range
        start_unicode_value: BigEndian<Uint24>,
        /// Number of additional values in this range
        additional_count: BigEndian<u8>,
    }
}
