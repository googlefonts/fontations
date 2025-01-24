#![parse_module(read_fonts::tables::vhea)]

/// The [vhea](https://docs.microsoft.com/en-us/typography/opentype/spec/vhea) Vertical Header Table
#[tag = "vhea"]
table Vhea {
    /// The major/minor version (1, 1)
    #[compile(Version16Dot16::VERSION_1_1)]
    version: Version16Dot16,
    /// Typographic ascent.
    ascender: FWord,
    /// Typographic descent.
    descender: FWord,
    /// Typographic line gap. Negative LineGap values are treated as
    /// zero in some legacy platform implementations.
    line_gap: FWord,
    /// Maximum advance height value in 'vmtx' table.
    advance_height_max: UfWord,
    /// Minimum top sidebearing value in 'vmtx' table for glyphs with
    /// contours (empty glyphs should be ignored).
    min_top_side_bearing: FWord,
    /// Minimum bottom sidebearing value
    min_bottom_side_bearing: FWord,
    /// Defined as max( tsb + (yMax-yMin)).
    y_max_extent: FWord,
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
    /// Number of advance heights in the vertical metrics (`vmtx`) table.
    number_of_long_ver_metrics: u16,
}
