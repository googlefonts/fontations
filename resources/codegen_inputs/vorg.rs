#![parse_module(read_fonts::tables::vorg)]

/// The [VORG (Vertical Origin)](https://docs.microsoft.com/en-us/typography/opentype/spec/vorg) table.
#[tag = "VORG"]
table Vorg {
    /// Major/minor version number. Set to 1.0.
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// The y coordinate of a glyph’s vertical origin, in the font’s design
    /// coordinate system, to be used if no entry is present for the glyph
    /// in the vertOriginYMetrics array.
    default_vert_origin_y: i16,
    /// Number of elements in the vertOriginYMetrics array.
    #[compile(array_len($vert_origin_y_metrics))]
    num_vert_origin_y_metrics: u16,
    /// Array of VertOriginYMetrics records, sorted by glyph ID.
    #[count($num_vert_origin_y_metrics)]
    vert_origin_y_metrics: [VertOriginYMetrics],
}

/// Vertical origin Y metrics record.
record VertOriginYMetrics {
    /// Glyph index.
    glyph_index: GlyphId16,
    /// Y coordinate, in the font’s design coordinate system, of the glyph’s vertical origin.
    vert_origin_y: i16,
}
