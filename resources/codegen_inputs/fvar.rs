#![parse_module(read_fonts::tables::fvar)]

extern record InstanceRecord;

/// The [fvar (Font Variations)](https://docs.microsoft.com/en-us/typography/opentype/spec/fvar) table
#[tag = "fvar"]
table Fvar {
    /// Major version number of the font variations table — set to 1.
    /// Minor version number of the font variations table — set to 0.
    version: MajorMinor,
    /// Offset in bytes from the beginning of the table to the start of the VariationAxisRecord array. The
    /// InstanceRecord array directly follows.
    #[read_offset_with($axis_count, $instance_count, $instance_size)]
    axis_instance_arrays_offset: Offset16<AxisInstanceArrays>,
    /// This field is permanently reserved. Set to 2.
    #[skip_getter]
    #[compile(2)]
    _reserved: u16,
    /// The number of variation axes in the font (the number of records in the axes array).
    axis_count: u16,
    /// The size in bytes of each VariationAxisRecord — set to 20 (0x0014) for this version.
    #[compile(20)]
    axis_size: u16,
    /// The number of named instances defined in the font (the number of records in the instances array).
    instance_count: u16,
    /// The size in bytes of each InstanceRecord — set to either axisCount * sizeof(Fixed) + 4, or to axisCount * sizeof(Fixed) + 6.
    #[compile(self.instance_size())]
    instance_size: u16,
}

/// Shim table to handle combined axis and instance arrays.
#[read_args(axis_count: u16, instance_count: u16, instance_size: u16)]
table AxisInstanceArrays {
    /// Variation axis record array.
    #[count($axis_count)]
    axes: [VariationAxisRecord],
    /// Instance record array.
    #[count($instance_count)]
    #[read_with($axis_count, $instance_size)]
    instances: ComputedArray<InstanceRecord<'a>>,
}

/// The [VariationAxisRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#variationaxisrecord)
record VariationAxisRecord {
    /// Tag identifying the design variation for the axis.
    axis_tag: Tag,
    /// The minimum coordinate value for the axis.
    min_value: Fixed,
    /// The default coordinate value for the axis.
    default_value: Fixed,
    /// The maximum coordinate value for the axis.
    max_value: Fixed,
    /// Axis qualifiers — see details below.
    flags: u16,
    /// The name ID for entries in the 'name' table that provide a display name for this axis.
    axis_name_id: NameId,
}
