#![parse_module(read_fonts::tables::avar)]

/// The [avar (Axis Variations)](https://docs.microsoft.com/en-us/typography/opentype/spec/avar) table
#[tag = "avar"]
table Avar {
    /// Major version number of the axis variations table — set to 1.
    /// Minor version number of the axis variations table — set to 0.
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Permanently reserved; set to zero.
    #[skip_getter]
    #[compile(0)]
    _reserved: u16,
    /// The number of variation axes for this font. This must be the same number as axisCount in the 'fvar' table.
    #[compile(array_len($axis_segment_maps))]
    axis_count: u16,
    /// The segment maps array — one segment map for each axis, in the order of axes specified in the 'fvar' table.
    #[count(..)]
    axis_segment_maps: VarLenArray<SegmentMaps<'a>>,
}

/// [SegmentMaps](https://learn.microsoft.com/en-us/typography/opentype/spec/avar#table-formats) record
record SegmentMaps<'a> {
    /// The number of correspondence pairs for this axis.
    #[compile(array_len($axis_value_maps))]
    position_map_count: u16,
    /// The array of axis value map records for this axis.
    #[count($position_map_count)]
    axis_value_maps: [AxisValueMap],
}

/// [AxisValueMap](https://learn.microsoft.com/en-us/typography/opentype/spec/avar#table-formats) record
record AxisValueMap {
    /// A normalized coordinate value obtained using default normalization.
    from_coordinate: F2Dot14,
    /// The modified, normalized coordinate value.
    to_coordinate: F2Dot14,
}
