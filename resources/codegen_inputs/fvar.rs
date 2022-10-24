#![parse_module(read_fonts::tables::fvar)]

/// [fvar (Font Variations Table)](https://learn.microsoft.com/en-us/typography/opentype/spec/fvar) table
table Fvar {
    /// Major version number of the font variations table — set to 1.
    major_version: BigEndian<u16>,
    /// Minor version number of the font variations table — set to 0.
    minor_version: BigEndian<u16>,
    /// Offset in bytes from the beginning of the table to the start of 
    /// the VariationAxisRecord array.
    #[read_offset_with($axis_count, $instance_count)]
    axes_array_offset: BigEndian<Offset16<FvarData>>,
    /// This field is permanently reserved. Set to 2.
    #[skip_getter]
    reserved: BigEndian<u16>,
    /// The number of variation axes in the font (the number of records 
    /// in the axes array).
    axis_count: BigEndian<u16>,
    /// The size in bytes of each VariationAxisRecord — set to 20 
    /// (0x0014) for this version.
    axis_size: BigEndian<u16>,
    /// The number of named instances defined in the font (the number 
    /// of records in the instances array).
    instance_count: BigEndian<u16>,
    /// The size in bytes of each InstanceRecord — set to either 
    /// axisCount * sizeof(Fixed) + 4, or to axisCount * sizeof(Fixed) 
    /// + 6.
    instance_size: BigEndian<u16>,
}

/// The header is followed by axes and instances arrays. The location of the axes array is specified in the axesArrayOffset field; the instances array directly follows the axes array.
#[read_args(axis_count: u16, instance_count: u16)]
record FvarData {
    /// The variation axis array.
    #[count($axis_count)]    
    axes: [VariationAxisRecord],
    /// The named instance array.
    #[count($instance_count)]    
    instances: [InstanceRecord],
}

record VariationAxisRecord {
    ///         Tag identifying the design variation for the axis.
    axis_tag: BigEndian<Tag>,
    ///     The minimum coordinate value for the axis.
    min_value: BigEndian<Fixed>,
    /// The default coordinate value for the axis.
    default_value: BigEndian<Fixed>,
    ///     The maximum coordinate value for the axis.
    max_value: BigEndian<Fixed>,
    ///         Axis qualifiers — see details below.
    flags: BigEndian<u16>,
    ///     The name ID for entries in the 'name' table that provide a 
    /// display name for this axis.
    axis_name_i_d: BigEndian<u16>,
}

record InstanceRecord {
    /// Coordinate array specifying a position within the font’s 
    /// variation space.
    #[count($axis_count)]
    coordinates: [BigEndian<Fixed>],
}

record InstanceRecord {
    /// The name ID for entries in the 'name' table that provide 
    /// subfamily names for this instance.
    subfamily_name_i_d: BigEndian<u16>,
    /// Reserved for future use — set to 0.
    flags: BigEndian<u16>,
    /// The coordinates array for this instance.
    coordinates: UserTuple,
    /// Optional. The name ID for entries in the 'name' table that 
    /// provide PostScript names for this instance.
    post_script_name_i_d: BigEndian<u16>,
}

record UserTuple {
    /// Coordinate array specifying a position within the font’s 
    /// variation space.
    #[count($axis_count)]
    coordinates: [BigEndian<Fixed>],
}

