#![parse_module(read_fonts::tables::hhea)]

/// [hhea](https://docs.microsoft.com/en-us/typography/opentype/spec/hhea) Horizontal Header Table
#[tag = "hhea"]
table Hhea {
    /// The major/minor version (1, 0)
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Typographic ascent.
    ascender: FWord,
    /// Typographic descent.
    descender: FWord,
    /// Typographic line gap. Negative LineGap values are treated as
    /// zero in some legacy platform implementations.
    line_gap: FWord,
    /// Maximum advance width value in 'hmtx' table.
    advance_width_max: UfWord,
    /// Minimum left sidebearing value in 'hmtx' table for glyphs with
    /// contours (empty glyphs should be ignored).
    min_left_side_bearing: FWord,
    /// Minimum right sidebearing value; calculated as min(aw - (lsb +
    /// xMax - xMin)) for glyphs with contours (empty glyphs should be ignored).
    min_right_side_bearing: FWord,
    /// Max(lsb + (xMax-xMin))
    x_max_extent: FWord,
    /// Used to calculate the slope of the cursor (rise/run); 1 for
    /// vertical caret, 0 for horizontal.
    caret_slope_rise: i16,
    /// 0 for vertical caret, 1 for horizontal.
    caret_slope_run: i16,
    /// The amount by which a slanted highlight on a glyph needs to be
    /// shifted to produce the best appearance. Set to 0 for
    /// non-slanted fonts
    caret_offset: i16,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved1: i16,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved2: i16,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved3: i16,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved4: i16,
    /// 0 for current format.
    #[compile(0)]
    metric_data_format: i16,
    /// Number of hMetric entries in 'hmtx' table
    number_of_h_metrics: u16,
}
