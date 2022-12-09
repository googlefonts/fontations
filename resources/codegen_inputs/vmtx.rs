#![parse_module(read_fonts::tables::vmtx)]

extern record LongMetric;

/// The [vmtx (Vertical Metrics)](https://docs.microsoft.com/en-us/typography/opentype/spec/vmtx) table
#[read_args(number_of_long_ver_metrics: u16, num_glyphs: u16)]
table Vmtx {
    /// Paired advance height and top side bearing values for each
    /// glyph. Records are indexed by glyph ID.
    #[count($number_of_long_ver_metrics)]
    v_metrics: [LongMetric],
    /// Top side bearings for glyph IDs greater than or equal to numberOfLongMetrics.
    #[count($num_glyphs.saturating_sub($number_of_long_ver_metrics) as usize)]
    top_side_bearings: [i16],
}
