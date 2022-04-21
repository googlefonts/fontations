/// The [hmtx (Horizontal Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx) table
#[read_args(number_of_h_metrics = "u16", num_glyphs = "u16")]
Hmtx<'a> {
    /// Paired advance width and left side bearing values for each
    /// glyph. Records are indexed by glyph ID.
    #[count(number_of_h_metrics)]
    h_metrics: [LongHorMetric],
    /// Left side bearings for glyph IDs greater than or equal to
    /// numberOfHMetrics.
    #[count_with(n_glyphs_less_n_metrics, num_glyphs, number_of_h_metrics)]
    left_side_bearings: [BigEndian<i16>],
}

LongHorMetric {
    /// Advance width, in font design units.
    advance_width: BigEndian<u16>,
    /// Glyph left side bearing, in font design units.
    lsb: BigEndian<i16>,
}

fn n_glyphs_less_n_metrics(num_glyphs: u16, num_metrics: u16) -> usize {
    num_glyphs.saturating_sub(num_metrics) as usize
}
