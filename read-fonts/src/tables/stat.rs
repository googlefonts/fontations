//! The [STAT](https://learn.microsoft.com/en-us/typography/opentype/spec/stat) table

use font_types::Tag;

/// 'STAT'
pub const TAG: Tag = Tag::new(b"STAT");

include!("../../generated/generated_stat.rs");

const KNOWN_AXIS_RECORD_SIZE: usize = Tag::RAW_BYTE_LEN + u16::RAW_BYTE_LEN + u16::RAW_BYTE_LEN;

#[derive(Debug, Clone)]
pub struct PaddingCalculator;

impl ReadArgs for PaddingCalculator {
    type Args = u16;
}

impl ComputeSize for PaddingCalculator {
    fn compute_size(args: &Self::Args) -> usize {
        dbg!(*args as usize) - KNOWN_AXIS_RECORD_SIZE
    }
}

impl FontReadWithArgs<'_> for PaddingCalculator {
    fn read_with_args(_: FontData, _: &Self::Args) -> Result<Self, ReadError> {
        Ok(PaddingCalculator)
    }
}

#[cfg(test)]
mod tests {
    use font_types::Fixed;

    use super::*;
    use crate::test_data::stat as test_data;

    #[test]
    fn smoke_test() {
        let table = test_data::vazirmatn();
        assert_eq!(table.design_axis_count(), 1);
        let axis_record = &table.design_axes().unwrap().get(0).unwrap();
        assert_eq!(axis_record.axis_tag(), Tag::new(b"wght"));
        assert_eq!(axis_record.axis_name_id(), 257);
        assert_eq!(axis_record.axis_ordering(), 0);
        let axis_values = table.offset_to_axis_values().unwrap();
        let axis_values = axis_values
            .axis_values()
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(axis_values.len(), 3);
        let last = &axis_values[2];
        if let AxisValue::Format1(table) = last {
            assert_eq!(table.axis_index(), 0);
            assert_eq!(table.value_name_id(), 264);
            assert_eq!(table.value(), Fixed::from_f64(700.0));
        }
    }
}
