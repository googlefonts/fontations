#![parse_module(read_fonts::tables::hvmtx)]

/// The [hmtx (Horizontal Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx) table
/// The [vmtx (Vertical Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/vmtx) table
#[read_args(number_of_long_metrics: u16, num_glyphs: u16)]
table HVmtx {
    /// Paired advance width/height and left/top side bearing values for each
    /// glyph. Records are indexed by glyph ID.
    #[count($number_of_long_metrics)]
    long_metrics: [LongMetric],
    /// Leading (left/top) side bearings for glyph IDs greater than or equal to
    /// numberOfLongMetrics.
    #[count($num_glyphs.saturating_sub($number_of_long_metrics) as usize)]
    bearings: [i16],
}

record LongMetric {
    /// Advance width/height, in font design units.
    advance: u16,
    /// Glyph leading (left/top) side bearing, in font design units.
    side_bearing: i16,
}

