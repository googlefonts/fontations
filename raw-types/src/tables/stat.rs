use raw_types::Offset16;

use crate::Uint16;

toy_table_macro::tables! {
    Stat<'a> {
        /// Major version number of the style attributes table — set to 1.
        major_version: Uint16,
        /// Minor version number of the style attributes table — set to 2.
        minor_version: Uint16,
        /// The size in bytes of each axis record.
        design_axis_size: Uint16,
        /// The number of axis records. In a font with an 'fvar' table,
        /// this value must be greater than or equal to the axisCount value
        /// in the 'fvar' table. In all fonts, must be greater than zero if
        /// axisValueCount is greater than zero.
        design_axis_count: Uint16,
        /// Offset in bytes from the beginning of the STAT table to the
        /// start of the design axes array. If designAxisCount is zero, set
        /// to zero; if designAxisCount is greater than zero, must be
        /// greater than zero.
        design_axes_offset: Offset32,
        /// The number of axis value tables.
        axis_value_count: Uint16,
        /// Offset in bytes from the beginning of the STAT table to the
        /// start of the design axes value offsets array. If axisValueCount
        /// is zero, set to zero; if axisValueCount is greater than zero,
        /// must be greater than zero.
        offset_to_axis_value_offsets: Offset32,
        /// Name ID used as fallback when projection of names into a
        /// particular font model produces a subfamily name containing only
        /// elidable elements.
        elided_fallback_name_id: Uint16,
        /// The design-axes array.
        #[count(design_axis_count)]
        design_axes: [AxisRecord],
        /// Array of offsets to axis value tables, in bytes from the start
        /// of the axis value offsets array.
        #[count(axis_value_count)]
        axis_value_offsets: [Offset16],
    }

    AxisRecord {
        /// A tag identifying the axis of design variation.
        axis_tag: Tag,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this axis.
        axis_name_i_d: Uint16,
        /// A value that applications can use to determine primary sorting
        /// of face names, or for ordering of labels when composing family
        /// or face names.
        axis_ordering: Uint16,
    }
}

toy_table_macro::tables! {
    AxisValueFormat1 {
        /// Format identifier — set to 1.
        format: Uint16,
        /// Zero-base index into the axis record array identifying the axis
        /// of design variation to which the axis value table applies. Must
        /// be less than designAxisCount.
        axis_index: Uint16,
        /// Flags — see below for details.
        flags: Uint16,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this attribute value.
        value_name_i_d: Uint16,
        /// A numeric value for this attribute value.
        value: Fixed,
    }

    AxisValueFormat2 {
        /// Format identifier — set to 2.
        format: Uint16,
        /// Zero-base index into the axis record array identifying the axis
        /// of design variation to which the axis value table applies. Must
        /// be less than designAxisCount.
        axis_index: Uint16,
        /// Flags — see below for details.
        flags: Uint16,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this attribute value.
        value_name_id: Uint16,
        /// A nominal numeric value for this attribute value.
        nominal_value: Fixed,
        /// The minimum value for a range associated with the specified
        /// name ID.
        range_min_value: Fixed,
        /// The maximum value for a range associated with the specified
        /// name ID.
        range_max_value: Fixed,
    }

    AxisValueFormat3 {
        /// Format identifier — set to 3.
        format: Uint16,
        /// Zero-base index into the axis record array identifying the axis
        /// of design variation to which the axis value table applies. Must
        /// be less than designAxisCount.
        axis_index: Uint16,
        /// Flags — see below for details.
        flags: Uint16,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this attribute value.
        value_name_id: Uint16,
        /// A numeric value for this attribute value.
        value: Fixed,
        /// The numeric value for a style-linked mapping from this value.
        linked_value: Fixed,
    }

    AxisValueFormat4<'a> {
        /// Format identifier — set to 4.
        format: Uint16,
        /// The total number of axes contributing to this axis-values
        /// combination.
        axis_count: Uint16,
        /// Flags — see below for details.
        flags: Uint16,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this combination of axis values.
        value_name_id: Uint16,
        /// Array of AxisValue records that provide the combination of axis
        /// values, one for each contributing axis.
        #[count(axis_count)]
        axis_values: [AxisValue],
    }

    #[format(Uint16)]
    enum AxisValueFormat<'a> {
        #[version(AxisValueFormat::FORMAT_1)]
        Format1(AxisValueFormat1),
        #[version(AxisValueFormat::FORMAT_2)]
        Format2(AxisValueFormat2),
        #[version(AxisValueFormat::FORMAT_3)]
        Format3(AxisValueFormat3),
        #[version(AxisValueFormat::FORMAT_4)]
        Format4(AxisValueFormat4<'a>),
    }

    AxisValue {
        /// Zero-base index into the axis record array identifying the axis
        /// to which this value applies. Must be less than designAxisCount.
        axis_index: Uint16,
        /// A numeric value for this attribute value.
        value: Fixed,
    }
}

impl AxisValueFormat<'_> {
    const FORMAT_1: Uint16 = Uint16::from_bytes(1u16.to_be_bytes());
    const FORMAT_2: Uint16 = Uint16::from_bytes(2u16.to_be_bytes());
    const FORMAT_3: Uint16 = Uint16::from_bytes(3u16.to_be_bytes());
    const FORMAT_4: Uint16 = Uint16::from_bytes(4u16.to_be_bytes());
}
