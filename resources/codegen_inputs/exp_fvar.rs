#![parse_module(read_fonts::exp::tables::fvar)]

// fvar, written for the reworked framework.
//
// This exists separately from `fvar.rs` because that input declares
// `extern record InstanceRecord` — the record is hand-written today, and the
// point of this file is to find out whether it still has to be.
//
// Two things kept it out of codegen:
//
// 1. its size is whatever `instanceSize` says, which the spec allows to exceed
//    the fields it holds, so it cannot be worked out by adding them up;
// 2. `postScriptNameID` is present only when `instanceSize` leaves room for it,
//    which is a condition on the field's own extent rather than on some other
//    field's value.
//
// `#[record_size(..)]` and `#[if_fits]` say both of those directly.

/// The [fvar (Font Variations)](https://docs.microsoft.com/en-us/typography/opentype/spec/fvar) table
///
/// Note what is *not* here: the `AxisInstanceArrays` shim. fvar points at two
/// consecutive arrays with a single offset, which codegen could not express, so
/// a table was invented to be the offset's target. `#[at_offset(..)]` says it
/// directly: the axis array starts where the offset points, and the instance
/// array follows it.
#[tag = "fvar"]
table Fvar {
    /// Major/minor version of the font variations table — set to 1.0.
    version: MajorMinor,
    /// Offset in bytes from the beginning of the table to the start of the
    /// VariationAxisRecord array. The InstanceRecord array directly follows.
    /// The offset is used positionally by the two arrays below rather than
    /// resolved on its own, so its generated resolver is suppressed in favour
    /// of [axes].
    #[offset_getter(axes)]
    axis_instance_arrays_offset: Offset16<[VariationAxisRecord]>,
    /// Permanently reserved. Set to 2.
    #[skip_getter]
    #[compile(2)]
    _reserved: u16,
    /// The number of variation axes in the font.
    axis_count: u16,
    /// The size in bytes of each VariationAxisRecord.
    axis_size: u16,
    /// The number of named instances defined in the font.
    instance_count: u16,
    /// The size in bytes of each InstanceRecord.
    instance_size: u16,
    /// Variation axis record array, at the offset above.
    #[at_offset($axis_instance_arrays_offset)]
    #[count($axis_count)]
    axes: [VariationAxisRecord],
    /// Instance record array, directly following the axes.
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
    /// Axis qualifiers.
    flags: u16,
    /// The name ID for entries in the 'name' table that provide a display name
    /// for this axis.
    axis_name_id: NameId,
}

/// The [InstanceRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#instancerecord)
#[read_args(axis_count: u16, instance_size: u16)]
#[record_size($instance_size)]
record InstanceRecord<'a> {
    /// The name ID for entries in the 'name' table that provide subfamily names
    /// for this instance.
    subfamily_name_id: NameId,
    /// Reserved for future use — set to 0.
    flags: u16,
    /// The coordinates array for this instance.
    #[count($axis_count)]
    coordinates: [Fixed],
    /// Optional. The name ID for entries in the 'name' table that provide
    /// PostScript names for this instance.
    #[if_fits]
    post_script_name_id: NameId,
}
