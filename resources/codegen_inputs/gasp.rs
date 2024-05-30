#![parse_module(read_fonts::tables::gasp)]

/// [gasp](https://learn.microsoft.com/en-us/typography/opentype/spec/gasp#gasp-table-formats)
table Gasp {
    /// Version number (set to 1)
    version: u16,
    /// Number of records to follow
    num_ranges: u16,
    /// Sorted by ppem
    #[count($num_ranges)]
    gasp_ranges: [GaspRange],
}

record GaspRange {
    /// Upper limit of range, in PPEM
    range_max_ppem: u16,
    /// Flags describing desired rasterizer behavior.
    range_gasp_behavior: GaspRangeBehavior,
}

flags u16 GaspRangeBehavior {
    /// Use gridfitting
    GASP_GRIDFIT = 0x0001,
    /// Use grayscale rendering
    GASP_DOGRAY = 0x0002,
    /// Use gridfitting with ClearType symmetric smoothing Only 
    /// supported in version 1 'gasp'
    GASP_SYMMETRIC_GRIDFIT = 0x0004,
    /// Use smoothing along multiple axes with ClearType® Only 
    /// supported in version 1 'gasp'
    GASP_SYMMETRIC_SMOOTHING = 0x0008,
}

