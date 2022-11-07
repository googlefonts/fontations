#![parse_module(read_fonts::tables::fvar)]


/// This is not tremendously friendly to codegen so we model it manually.
extern record FvarData;

/// [fvar (Font Variations Table)](https://learn.microsoft.com/en-us/typography/opentype/spec/fvar) table
table Fvar {
    /// Major version number of the font variations table — set to 1.
    major_version: u16,
    /// Minor version number of the font variations table — set to 0.
    minor_version: u16,
    /// Offset in bytes from the beginning of the table to the start of 
    /// the VariationAxisRecord array.
    /// The header is followed by axes and instances arrays.
    /// The location of the axes array is specified in the axesArrayOffset field;
    /// the instances array directly follows the axes array.
    #[read_offset_with($axis_count, $instance_count)]
    axes_array_offset: Offset16<FvarData>,
    /// This field is permanently reserved. Set to 2.
    #[skip_getter]
    reserved: u16,
    /// The number of variation axes in the font (the number of records 
    /// in the axes array).
    axis_count: u16,
    /// The size in bytes of each VariationAxisRecord — set to 20 
    /// (0x0014) for this version.
    axis_size: u16,
    /// The number of named instances defined in the font (the number 
    /// of records in the instances array).
    instance_count: u16,
    /// The size in bytes of each InstanceRecord — set to either 
    /// axisCount * sizeof(Fixed) + 4, or to axisCount * sizeof(Fixed) 
    /// + 6.
    instance_size: u16,
}

record VariationAxisRecord {
    ///         Tag identifying the design variation for the axis.
    axis_tag: Tag,
    ///     The minimum coordinate value for the axis.
    min_value: Fixed,
    /// The default coordinate value for the axis.
    default_value: Fixed,
    ///     The maximum coordinate value for the axis.
    max_value: Fixed,
    ///         Axis qualifiers — see details below.
    flags: u16,
    ///     The name ID for entries in the 'name' table that provide a 
    /// display name for this axis.
    axis_name_id: u16,
}

#[read_args(axis_count: u16)]
record InstanceRecord {
    /// The name ID for entries in the 'name' table that provide 
    /// subfamily names for this instance.
    subfamily_name_i_d: u16,
    /// Reserved for future use — set to 0.
    flags: u16,
    /// The coordinates array for this instance.
    #[read_with($axis_count)]
    coordinates: UserTuple,
    /// Optional. The name ID for entries in the 'name' table that 
    /// provide PostScript names for this instance.
    post_script_name_i_d: u16,
}

#[read_args(axis_count: u16)]
record UserTuple<'a> {
    /// Coordinate array specifying a position within the font’s 
    /// variation space.
    #[count($axis_count)]
    coordinates: [Fixed],
}

