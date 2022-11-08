#![parse_module(read_fonts::tables::stat)]

/// [STAT](https://docs.microsoft.com/en-us/typography/opentype/spec/stat) (Style Attributes Table)
table Stat {
    /// Major/minor version number. Set to 1.2 for new fonts.
    #[version]
    #[compile(MajorMinor::VERSION_1_2)]
    version: MajorMinor,
    /// The size in bytes of each axis record.
    #[compile(8)]
    design_axis_size: u16,
    /// The number of axis records. In a font with an 'fvar' table,
    /// this value must be greater than or equal to the axisCount value
    /// in the 'fvar' table. In all fonts, must be greater than zero if
    /// axisValueCount is greater than zero.
    #[compile(array_len($offset_to_axis_value_offsets))]
    design_axis_count: u16,
    /// Offset in bytes from the beginning of the STAT table to the
    /// start of the design axes array. If designAxisCount is zero, set
    /// to zero; if designAxisCount is greater than zero, must be
    /// greater than zero.
    #[read_offset_with($design_axis_count)]
    design_axes_offset: Offset32<[AxisRecord]>,
    /// The number of axis value tables.
    #[compile(array_len($offset_to_axis_value_offsets))]
    axis_value_count: u16,
    /// Offset in bytes from the beginning of the STAT table to the
    /// start of the design axes value offsets array. If axisValueCount
    /// is zero, set to zero; if axisValueCount is greater than zero,
    /// must be greater than zero.
    #[read_offset_with($axis_value_count)]
    #[compile_type(OffsetMarker<Vec<OffsetMarker<AxisValue>>, WIDTH_32>)]
    #[to_owned(convert_axis_value_offsets(obj.offset_to_axis_values()))]
    offset_to_axis_value_offsets: Offset32<AxisValueArray>,
    /// Name ID used as fallback when projection of names into a
    /// particular font model produces a subfamily name containing only
    /// elidable elements.
    #[available(MajorMinor::VERSION_1_1)]
    elided_fallback_name_id: u16,
}

/// [Axis Records](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-records)
record AxisRecord {
    /// A tag identifying the axis of design variation.
    axis_tag: Tag,
    /// The name ID for entries in the 'name' table that provide a
    /// display string for this axis.
    axis_name_id: u16,
    /// A value that applications can use to determine primary sorting
    /// of face names, or for ordering of labels when composing family
    /// or face names.
    axis_ordering: u16,
}

//NOTE: this is a shim table, because it would be annoying to support an offset
//to an array of offsets
/// An array of [AxisValue] tables.
#[read_args(axis_value_count: u16)]
table AxisValueArray {
    /// Array of offsets to axis value tables, in bytes from the start
    /// of the axis value offsets array.
    #[count($axis_value_count)]
    axis_value_offsets: [Offset16<AxisValue>],
}

/// [Axis Value Tables](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-tables)
format u16 AxisValue {
    Format1(AxisValueFormat1),
    Format2(AxisValueFormat2),
    Format3(AxisValueFormat3),
    Format4(AxisValueFormat4),
}

/// [Axis value table format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-1)
table AxisValueFormat1 {
    /// Format identifier — set to 1.
    #[format = 1]
    format: u16,
    /// Zero-base index into the axis record array identifying the axis
    /// of design variation to which the axis value table applies. Must
    /// be less than designAxisCount.
    axis_index: u16,
    /// Flags — see below for details.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the 'name' table that provide a
    /// display string for this attribute value.
    value_name_id: u16,
    /// A numeric value for this attribute value.
    value: Fixed,
}

/// [Axis value table format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-2)
table AxisValueFormat2 {
    /// Format identifier — set to 2.
    #[format = 2]
    format: u16,
    /// Zero-base index into the axis record array identifying the axis
    /// of design variation to which the axis value table applies. Must
    /// be less than designAxisCount.
    axis_index: u16,
    /// Flags — see below for details.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the 'name' table that provide a
    /// display string for this attribute value.
    value_name_id: u16,
    /// A nominal numeric value for this attribute value.
    nominal_value: Fixed,
    /// The minimum value for a range associated with the specified
    /// name ID.
    range_min_value: Fixed,
    /// The maximum value for a range associated with the specified
    /// name ID.
    range_max_value: Fixed,
}

/// [Axis value table format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-3)
table AxisValueFormat3 {
    /// Format identifier — set to 3.
    #[format = 3]
    format: u16,
    /// Zero-base index into the axis record array identifying the axis
    /// of design variation to which the axis value table applies. Must
    /// be less than designAxisCount.
    axis_index: u16,
    /// Flags — see below for details.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the 'name' table that provide a
    /// display string for this attribute value.
    value_name_id: u16,
    /// A numeric value for this attribute value.
    value: Fixed,
    /// The numeric value for a style-linked mapping from this value.
    linked_value: Fixed,
}

/// [Axis value table format 4](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-4)
table AxisValueFormat4 {
    /// Format identifier — set to 4.
    #[format = 4]
    format: u16,
    /// The total number of axes contributing to this axis-values
    /// combination.
    #[compile(array_len($axis_values))]
    axis_count: u16,
    /// Flags — see below for details.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the 'name' table that provide a
    /// display string for this combination of axis values.
    value_name_id: u16,
    /// Array of AxisValue records that provide the combination of axis
    /// values, one for each contributing axis.
    #[count($axis_count)]
    axis_values: [AxisValueRecord],
}

/// Part of [AxisValueFormat4]
record AxisValueRecord {
    /// Zero-base index into the axis record array identifying the axis
    /// to which this value applies. Must be less than designAxisCount.
    axis_index: u16,
    /// A numeric value for this attribute value.
    value: Fixed,
}

/// [Axis value table flags](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#flags).
flags u16 AxisValueTableFlags {
    /// If set, this axis value table provides axis value information
    /// that is applicable to other fonts within the same font family.
    /// This is used if the other fonts were released earlier and did
    /// not include information about values for some axis. If newer
    /// versions of the other fonts include the information themselves
    /// and are present, then this table is ignored.
    OLDER_SIBLING_FONT_ATTRIBUTE = 0x0001,
    /// If set, it indicates that the axis value represents the
    /// “normal” value for the axis and may be omitted when
    /// composing name strings.
    ELIDABLE_AXIS_VALUE_NAME = 0x0002,
}

