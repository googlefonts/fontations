#![parse_module(font_tables::tables::hmtx)]

/// The [hmtx (Horizontal Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx) table
#[read_args(number_of_h_metrics: u16, num_glyphs: u16)]
table Hmtx {
    /// Paired advance width and left side bearing values for each
    /// glyph. Records are indexed by glyph ID.
    #[count($number_of_h_metrics)]
    h_metrics: [LongHorMetric],
    /// Left side bearings for glyph IDs greater than or equal to
    /// numberOfHMetrics.
    #[count($num_glyphs.saturating_sub($number_of_h_metrics) as usize)]
    left_side_bearings: [BigEndian<i16>],
}

record LongHorMetric {
    /// Advance width, in font design units.
    advance_width: BigEndian<u16>,
    /// Glyph left side bearing, in font design units.
    lsb: BigEndian<i16>,
}

