//! the [STAT] table
//!
//! [STAT]: https://docs.microsoft.com/en-us/typography/opentype/spec/stat

#[path = "../../generated/generated_stat.rs"]
mod generated;

pub use generated::*;

use font_types::{BigEndian, FontRead, Offset, Offset16, OffsetHost, Tag};
use zerocopy::LayoutVerified;

/// 'STAT'
pub const TAG: Tag = Tag::new(b"STAT");

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

#[cfg(test)]
mod tests {
    use font_types::{Fixed, Offset32};

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
