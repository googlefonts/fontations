#![parse_module(read_fonts::tables::bitmap)]

/// [BitmapSize](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#bitmapsize-record) record.
record BitmapSize {
    /// Offset to IndexSubtableList, from beginning of EBLC/CBLC.
    index_subtable_list_offset: u32,
    /// Total size in bytes of the IndexSubtableList including its array of IndexSubtables.
    index_subtable_list_size: u32,
    /// Number of IndexSubtables in the IndexSubtableList.
    number_of_index_subtables: u32,
    /// Not used; set to 0.
    color_ref: u32,
    /// Line metrics for text rendered horizontally.
    hori: SbitLineMetrics,
    /// Line metrics for text rendered vertically.
    vert: SbitLineMetrics,
    /// Lowest glyph index for this size.
    start_glyph_index: GlyphId16,
    /// Highest glyph index for this size.
    end_glyph_index: GlyphId16,
    /// Horizontal pixels per em.
    ppem_x: u8,
    /// Vertical pixels per em.
    ppem_y: u8,
    /// The Microsoft rasterizer v.1.7 or greater supports the following
    /// bitDepth values, as described below: 1, 2, 4, and 8 (and 32 for CBLC).
    bit_depth: u8,
    /// Vertical or horizontal.
    flags: BitmapFlags,
}

/// [SbitLineMetrics](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#sbitlinemetrics-record) record.
record SbitLineMetrics {
    ascender: i8,
    descender: i8,
    width_max: u8,
    caret_slope_numerator: i8,
    caret_slope_denominator: u8,
    caret_offset: i8,
    min_origin_sb: i8,
    min_advance_sb: i8,
    max_before_bl: i8,
    min_after_bl: i8,
    pad1: i8,
    pad2: i8,
}

/// [Bitmap flags](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#bitmap-flags).
flags u8 BitmapFlags {
    /// Horizontal
    HORIZONTAL_METRICS = 0x01,
    /// Vertical
    VERTICAL_METRICS = 0x02,
}

/// [BigGlyphMetrics](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#bigglyphmetrics) record.
record BigGlyphMetrics {
    /// Number of rows of data.
    height: u8,
    /// Number of columns of data.
    width: u8,
    /// Distance in pixels from the horizontal origin to the left edge of the bitmap.
    hori_bearing_x: i8,
    /// Distance in pixels from the horizontal origin to the top edge of the bitmap.
    hori_bearing_y: i8,
    /// Horizontal advance width in pixels.
    hori_advance: u8,
    /// Distance in pixels from the vertical origin to the left edge of the bitmap.
    vert_bearing_x: i8,
    /// Distance in pixels from the vertical origin to the top edge of the bitmap.
    vert_bearing_y: i8,
    /// Vertical advance width in pixels.
    vert_advance: u8,
}

/// [SmallGlyphMetrics](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#smallglyphmetrics) record.
record SmallGlyphMetrics {
    /// Number of rows of data.
    height: u8,
    /// Number of columns of data.
    width: u8,
    /// Distance in pixels from the horizontal origin to the left edge of the bitmap (for horizontal text); or distance in pixels from the vertical origin to the top edge of the bitmap (for vertical text).
    bearing_x: i8,
    /// Distance in pixels from the horizontal origin to the top edge of the bitmap (for horizontal text); or distance in pixels from the vertical origin to the left edge of the bitmap (for vertical text).
    bearing_y: i8,
    /// Horizontal or vertical advance width in pixels.
    advance: u8,
}

/// [IndexSubtableList](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#indexsubtablelist) table.
#[read_args(number_of_index_subtables: u32)]
table IndexSubtableList {
    /// Array of IndexSubtableRecords.
    #[count($number_of_index_subtables)]
    index_subtable_records: [IndexSubtableRecord],
}

record IndexSubtableRecord {
    /// First glyph ID of this range.
    first_glyph_index: GlyphId16,
    /// Last glyph ID of this range (inclusive).
    last_glyph_index: GlyphId16,
    /// Offset to an IndexSubtable from the start of the IndexSubtableList.
    #[read_offset_with($last_glyph_index, $first_glyph_index)]
    index_subtable_offset: Offset32<IndexSubtable>,
}

/// [IndexSubTable1](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#indexsubtable1-variable-metrics-glyphs-with-4-byte-offsets): variable-metrics glyphs with 4-byte offsets.
#[read_args(last_glyph_index: GlyphId16, first_glyph_index: GlyphId16)]
table IndexSubtable1 {
    /// Format of this IndexSubTable.
    #[format = 1]
    index_format: u16,
    /// Format of EBDT image data.
    image_format: u16,
    /// Offset to image data in EBDT table.
    image_data_offset: u32,
    #[count(subtract_add_two($last_glyph_index, $first_glyph_index))]
    sbit_offsets: [u32],
}

/// [IndexSubTable2](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#indexsubtable2-all-glyphs-have-identical-metrics): all glyphs have identical metrics.
table IndexSubtable2 {
    /// Format of this IndexSubTable.
    #[format = 2]
    index_format: u16,
    /// Format of EBDT image data.
    image_format: u16,
    /// Offset to image data in EBDT table.
    image_data_offset: u32,
    /// All the glyphs are of the same size.
    image_size: u32,
    /// All glyphs have the same metrics; glyph data may be compressed, byte-aligned, or bit-aligned.
    #[count(1)]
    big_metrics: [BigGlyphMetrics],
}

/// [IndexSubTable3](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#indexsubtable3-variable-metrics-glyphs-with-2-byte-offsets): variable-metrics glyphs with 2-byte offsets.
#[read_args(last_glyph_index: GlyphId16, first_glyph_index: GlyphId16)]
table IndexSubtable3 {
    /// Format of this IndexSubTable.
    #[format = 3]
    index_format: u16,
    /// Format of EBDT image data.
    image_format: u16,
    /// Offset to image data in EBDT table.
    image_data_offset: u32,
    #[count(subtract_add_two($last_glyph_index, $first_glyph_index))]
    sbit_offsets: [u16],
}

/// [IndexSubTable4](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#indexsubtable3-variable-metrics-glyphs-with-2-byte-offsets): variable-metrics glyphs with sparse glyph codes.
table IndexSubtable4 {
    /// Format of this IndexSubTable.
    #[format = 4]
    index_format: u16,
    /// Format of EBDT image data.
    image_format: u16,
    /// Offset to image data in EBDT table.
    image_data_offset: u32,
    /// Array length.
    num_glyphs: u32,
    /// One per glyph.
    #[count(add($num_glyphs, 1))]
    glyph_array: [GlyphIdOffsetPair],
}

/// [GlyphIdOffsetPair](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#glyphidoffsetpair-record) record.
record GlyphIdOffsetPair {
    /// Glyph ID of glyph present.
    glyph_id: GlyphId16,
    /// Location in EBDT.
    sbit_offset: u16,
}

/// [IndexSubTable5](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc#indexsubtable5-constant-metrics-glyphs-with-sparse-glyph-codes): constant-metrics glyphs with sparse glyph codes
table IndexSubtable5 {
    /// Format of this IndexSubTable.
    #[format = 5]
    index_format: u16,
    /// Format of EBDT image data.
    image_format: u16,
    /// Offset to image data in EBDT table.
    image_data_offset: u32,
    /// All glyphs have the same data size.
    image_size: u32,
    /// All glyphs have the same metrics.
    #[count(1)]
    big_metrics: [BigGlyphMetrics],
    /// Array length.
    num_glyphs: u32,
    /// One per glyph, sorted by glyhph ID.
    #[count($num_glyphs)]
    glyph_array: [GlyphId16],
}

/// [EbdtComponent](https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt#ebdtcomponent-record) record.
record BdtComponent {
    /// Component glyph ID.
    glyph_id: GlyphId16,
    /// Position of component left.
    x_offset: i8,
    /// Position of component top.
    y_offset: i8,
}
