//! the [STAT] table
//!
//! [STAT]: https://docs.microsoft.com/en-us/typography/opentype/spec/stat

use font_types::{
    BigEndian, Fixed, FontRead, MajorMinor, Offset, Offset16, Offset32, OffsetHost, Tag,
};
use zerocopy::LayoutVerified;

/// 'STAT'
pub const TAG: Tag = Tag::new(b"STAT");

font_types::tables! {
    /// [STAT](https://docs.microsoft.com/en-us/typography/opentype/spec/stat) (Style Attributes Table)
    #[offset_host]
    Stat1_0<'a> {
        /// Major version number of the style attributes table — set to 1.
        major_version: BigEndian<u16>,
        /// Minor version number of the style attributes table — set to 2.
        minor_version: BigEndian<u16>,
        /// The size in bytes of each axis record.
        design_axis_size: BigEndian<u16>,
        /// The number of axis records. In a font with an 'fvar' table,
        /// this value must be greater than or equal to the axisCount value
        /// in the 'fvar' table. In all fonts, must be greater than zero if
        /// axisValueCount is greater than zero.
        design_axis_count: BigEndian<u16>,
        /// Offset in bytes from the beginning of the STAT table to the
        /// start of the design axes array. If designAxisCount is zero, set
        /// to zero; if designAxisCount is greater than zero, must be
        /// greater than zero.
        design_axes_offset: BigEndian<Offset32>,
        /// The number of axis value tables.
        axis_value_count: BigEndian<u16>,
        /// Offset in bytes from the beginning of the STAT table to the
        /// start of the design axes value offsets array. If axisValueCount
        /// is zero, set to zero; if axisValueCount is greater than zero,
        /// must be greater than zero.
        offset_to_axis_value_offsets: BigEndian<Offset32>,
    }

    /// [STAT](https://docs.microsoft.com/en-us/typography/opentype/spec/stat) (Style Attributes Table)
    #[offset_host]
    Stat1_2<'a> {
        /// Major version number of the style attributes table — set to 1.
        major_version: BigEndian<u16>,
        /// Minor version number of the style attributes table — set to 2.
        minor_version: BigEndian<u16>,
        /// The size in bytes of each axis record.
        design_axis_size: BigEndian<u16>,
        /// The number of axis records. In a font with an 'fvar' table,
        /// this value must be greater than or equal to the axisCount value
        /// in the 'fvar' table. In all fonts, must be greater than zero if
        /// axisValueCount is greater than zero.
        design_axis_count: BigEndian<u16>,
        /// Offset in bytes from the beginning of the STAT table to the
        /// start of the design axes array. If designAxisCount is zero, set
        /// to zero; if designAxisCount is greater than zero, must be
        /// greater than zero.
        design_axes_offset: BigEndian<Offset32>,
        /// The number of axis value tables.
        axis_value_count: BigEndian<u16>,
        /// Offset in bytes from the beginning of the STAT table to the
        /// start of the design axes value offsets array. If axisValueCount
        /// is zero, set to zero; if axisValueCount is greater than zero,
        /// must be greater than zero.
        offset_to_axis_value_offsets: BigEndian<Offset32>,
        /// Name ID used as fallback when projection of names into a
        /// particular font model produces a subfamily name containing only
        /// elidable elements.
        elided_fallback_name_id: BigEndian<u16>,
    }

    /// [Axis Records](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-records)
    AxisRecord {
        /// A tag identifying the axis of design variation.
        axis_tag: BigEndian<Tag>,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this axis.
        axis_name_id: BigEndian<u16>,
        /// A value that applications can use to determine primary sorting
        /// of face names, or for ordering of labels when composing family
        /// or face names.
        axis_ordering: BigEndian<u16>,
    }

    /// [STAT](https://docs.microsoft.com/en-us/typography/opentype/spec/stat) (Style Attributes Table)
    #[format(MajorMinor)]
    #[generate_getters]
    enum Stat<'a> {
        #[version(MajorMinor::VERSION_1_0)]
        Version1_0(Stat1_0<'a>),
        #[version(MajorMinor::VERSION_1_2)]
        Version1_2(Stat1_2<'a>),
    }
}

//FIXME: we should generate this automatically?
impl<'a> OffsetHost<'a> for Stat<'a> {
    fn bytes(&self) -> &'a [u8] {
        match self {
            Stat::Version1_0(table) => table.bytes(),
            Stat::Version1_2(table) => table.bytes(),
        }
    }
}

impl<'a> Stat<'a> {
    /// The design-axes array.
    pub fn design_axes(&self) -> impl Iterator<Item = AxisRecord> + '_ {
        let count = self.design_axis_count();
        let offset = self.design_axes_offset();
        let record_len = self.design_axis_size() as usize;
        let bytes = self.bytes_at_offset(offset);
        let mut idx = 0;
        std::iter::from_fn(move || {
            if idx == count as usize {
                return None;
            }
            let rel_off = idx * record_len;
            let result = bytes
                .get(rel_off..rel_off + record_len)
                .and_then(AxisRecord::read);
            idx += 1;
            result
        })
    }

    fn axis_value_offsets(&self) -> &[BigEndian<Offset16>] {
        let count = self.axis_value_count();
        let offset = self.offset_to_axis_value_offsets();
        let bytes = self.bytes_at_offset(offset);
        match LayoutVerified::new_slice_unaligned_from_prefix(bytes, count as usize) {
            Some((layout, _)) => layout.into_slice(),
            None => &[],
        }
    }

    pub fn iter_axis_value_tables(&self) -> impl Iterator<Item = AxisValue<'a>> + '_ {
        let offset_start = self.offset_to_axis_value_offsets();
        let bytes = self.bytes_at_offset(offset_start);
        self.axis_value_offsets().iter().map_while(|off| {
            off.get()
                .non_null()
                .and_then(|off| bytes.get(off..).and_then(AxisValue::read))
        })
    }
}

font_types::tables! {

    /// [Axis Value Tables](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-tables)
    #[format(u16)]
    enum AxisValue<'a> {
        #[version(1)]
        Format1(AxisValueFormat1),
        #[version(2)]
        Format2(AxisValueFormat2),
        #[version(3)]
        Format3(AxisValueFormat3),
        #[version(4)]
        Format4(AxisValueFormat4<'a>),
    }

    /// [Axis value table format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-1)
    AxisValueFormat1 {
        /// Format identifier — set to 1.
        format: BigEndian<u16>,
        /// Zero-base index into the axis record array identifying the axis
        /// of design variation to which the axis value table applies. Must
        /// be less than designAxisCount.
        axis_index: BigEndian<u16>,
        /// Flags — see below for details.
        flags: BigEndian<AxisValueFlags>,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this attribute value.
        value_name_id: BigEndian<u16>,
        /// A numeric value for this attribute value.
        value: BigEndian<Fixed>,
    }

    /// [Axis value table format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-2)
    AxisValueFormat2 {
        /// Format identifier — set to 2.
        format: BigEndian<u16>,
        /// Zero-base index into the axis record array identifying the axis
        /// of design variation to which the axis value table applies. Must
        /// be less than designAxisCount.
        axis_index: BigEndian<u16>,
        /// Flags — see below for details.
        flags: BigEndian<AxisValueFlags>,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this attribute value.
        value_name_id: BigEndian<u16>,
        /// A nominal numeric value for this attribute value.
        nominal_value: BigEndian<Fixed>,
        /// The minimum value for a range associated with the specified
        /// name ID.
        range_min_value: BigEndian<Fixed>,
        /// The maximum value for a range associated with the specified
        /// name ID.
        range_max_value: BigEndian<Fixed>,
    }

    /// [Axis value table format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-3)
    AxisValueFormat3 {
        /// Format identifier — set to 3.
        format: BigEndian<u16>,
        /// Zero-base index into the axis record array identifying the axis
        /// of design variation to which the axis value table applies. Must
        /// be less than designAxisCount.
        axis_index: BigEndian<u16>,
        /// Flags — see below for details.
        flags: BigEndian<AxisValueFlags>,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this attribute value.
        value_name_id: BigEndian<u16>,
        /// A numeric value for this attribute value.
        value: BigEndian<Fixed>,
        /// The numeric value for a style-linked mapping from this value.
        linked_value: BigEndian<Fixed>,
    }

    /// [Axis value table format 4](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-4)
    AxisValueFormat4<'a> {
        /// Format identifier — set to 4.
        format: BigEndian<u16>,
        /// The total number of axes contributing to this axis-values
        /// combination.
        axis_count: BigEndian<u16>,
        /// Flags — see below for details.
        flags: BigEndian<AxisValueFlags>,
        /// The name ID for entries in the 'name' table that provide a
        /// display string for this combination of axis values.
        value_name_id: BigEndian<u16>,
        /// Array of AxisValue records that provide the combination of axis
        /// values, one for each contributing axis.
        #[count(axis_count)]
        axis_values: [AxisValueRecord],
    }

    /// Part of [AxisValueFormat4]
    AxisValueRecord {
        /// Zero-base index into the axis record array identifying the axis
        /// to which this value applies. Must be less than designAxisCount.
        axis_index: BigEndian<u16>,
        /// A numeric value for this attribute value.
        value: BigEndian<Fixed>,
    }

    /// [Axis value table flags](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#flags).
#[flags(u16)]
    AxisValueFlags {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        let mut buf = font_types::test_helpers::BeBuffer::new();
        buf.extend([1u16, 0]); // version
        buf.push(std::mem::size_of::<AxisRecord>() as u16); // design axis size
        buf.push(1u16); // design axis count
        buf.push(Offset32::new(18));
        buf.push(2u16);
        buf.push(Offset32::new(18 + std::mem::size_of::<AxisRecord>() as u32));
        assert_eq!(buf.len(), 18, "sanity check");
        // now push one axis record:
        buf.push(Tag::new(b"wght"));
        buf.extend([1u16, 1u16]);
        // and two value tables:

        //let axis_value_offset_start = buf.len();
        buf.push(4u16); // first offset
        buf.push(4u16 + std::mem::size_of::<AxisValueFormat1>() as u16); // first offset

        // format 1:
        buf.push(1u16);
        buf.push(9u16);
        buf.push(0u16);
        buf.push(2u16);
        buf.push(Fixed::from_f64(42.0));

        // format 3:
        buf.push(3u16);
        buf.push(0u16);
        buf.push(0u16);
        buf.push(7u16);
        buf.push(Fixed::from_f64(-3.3));
        buf.push(Fixed::from_f64(108.));

        let table = Stat::read(&buf).unwrap();
        assert_eq!(table.design_axis_count(), 1);
        assert_eq!(table.design_axis_count(), 1);
        assert_eq!(table.elided_fallback_name_id(), None);

        let values = table.iter_axis_value_tables().collect::<Vec<_>>();
        let value1 = match values[0] {
            AxisValue::Format1(table) => table,
            AxisValue::Format2(_) => panic!("format2"),
            AxisValue::Format3(_) => panic!("format3"),
            AxisValue::Format4(_) => panic!("format4"),
        };

        assert_eq!(value1.axis_index(), 9);
        assert_eq!(value1.value(), Fixed::from_f64(42.0));
        let value2 = match values[1] {
            AxisValue::Format3(table) => table,
            _ => panic!("unexpected format"),
        };

        assert_eq!(value2.value_name_id(), 7);
        assert_eq!(value2.linked_value(), Fixed::from_f64(108.));
    }
}
