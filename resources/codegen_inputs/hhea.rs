#![parse_module(read_fonts::tables::hhea)]

/// [hhea](https://docs.microsoft.com/en-us/typography/opentype/spec/hhea) Horizontal Header Table
table Hhea {
    /// The major/minor version (1, 0)
    #[compile(MajorMinor::VERSION_1_0)]
    version: BigEndian<MajorMinor>,
    /// Typographic ascent—see note below.
    ascender: BigEndian<FWord>,
    /// Typographic descent—see note below.
    descender: BigEndian<FWord>,
    /// Typographic line gap. Negative LineGap values are treated as
    /// zero in some legacy platform implementations.
    line_gap: BigEndian<FWord>,
    /// Maximum advance width value in 'hmtx' table.
    advance_width_max: BigEndian<UfWord>,
    /// Minimum left sidebearing value in 'hmtx' table for glyphs with
    /// contours (empty glyphs should be ignored).
    min_left_side_bearing: BigEndian<FWord>,
    /// Minimum right sidebearing value; calculated as min(aw - (lsb +
    /// xMax - xMin)) for glyphs with contours (empty glyphs should be
    /// ignored).
    min_right_side_bearing: BigEndian<FWord>,
    /// Max(lsb + (xMax - xMin)).
    x_max_extent: BigEndian<FWord>,
    /// Used to calculate the slope of the cursor (rise/run); 1 for
    /// vertical.
    caret_slope_rise: BigEndian<i16>,
    /// 0 for vertical.
    caret_slope_run: BigEndian<i16>,
    /// The amount by which a slanted highlight on a glyph needs to be
    /// shifted to produce the best appearance. Set to 0 for
    /// non-slanted fonts
    caret_offset: BigEndian<i16>,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved1: BigEndian<i16>,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved2: BigEndian<i16>,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved3: BigEndian<i16>,
    /// set to 0
    #[skip_getter]
    #[compile(0)]
    reserved4: BigEndian<i16>,
    /// 0 for current format.
    #[compile(0)]
    metric_data_format: BigEndian<i16>,
    /// Number of hMetric entries in 'hmtx' table
    number_of_h_metrics: BigEndian<u16>,
}
