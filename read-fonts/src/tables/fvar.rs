//! The [Font Variations](https://docs.microsoft.com/en-us/typography/opentype/spec/fvar) table

include!("../../generated/generated_fvar.rs");

#[path = "./instance_record.rs"]
mod instance_record;

pub use instance_record::InstanceRecord;

impl<'a> Fvar<'a> {
    /// Returns the array of variation axis records.
    pub fn axes(&self) -> Result<&'a [VariationAxisRecord], ReadError> {
        Ok(self.axis_instance_arrays()?.axes())
    }

    /// Returns the array of instance records.
    pub fn instances(&self) -> Result<ComputedArray<'a, InstanceRecord<'a>>, ReadError> {
        Ok(self.axis_instance_arrays()?.instances())
    }
}

impl VariationAxisRecord {
    /// Given a value in user space, normalize it for this axis.
    ///
    /// The value will be clamped to the axis's min/max values, then normalized;
    /// it will always be in the range `-1.0..=1.0`
    pub fn normalize(&self, value: Fixed) -> Fixed {
        normalize_coordinate_impl(
            value,
            self.min_value(),
            self.default_value(),
            self.max_value(),
        )
    }
}

/// Compute the normalized coordinate for a given value/axis min/max/default (in user space)
///
/// This is only exposed so that it can be reused in write-fonts
#[doc(hidden)]
pub fn normalize_coordinate_impl(
    coord: Fixed,
    min_value: Fixed,
    default_value: Fixed,
    max_value: Fixed,
) -> Fixed {
    use core::cmp::Ordering::*;
    // Make sure max is >= min to avoid potential panic in clamp.
    let max_value = max_value.max(min_value);
    let coord = coord.clamp(min_value, max_value);
    let coord = match coord.cmp(&default_value) {
        Less => -((default_value - coord) / (default_value - min_value)),
        Greater => (coord - default_value) / (max_value - default_value),
        Equal => Fixed::ZERO,
    };
    coord.clamp(-Fixed::ONE, Fixed::ONE)
}

#[cfg(test)]
mod tests {
    use crate::{FontRef, TableProvider};
    use types::{Fixed, NameId, Tag};

    #[test]
    fn axes() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let fvar = font.fvar().unwrap();
        assert_eq!(fvar.axis_count(), 1);
        let wght = &fvar.axes().unwrap().get(0).unwrap();
        assert_eq!(wght.axis_tag(), Tag::new(b"wght"));
        assert_eq!(wght.min_value(), Fixed::from_f64(100.0));
        assert_eq!(wght.default_value(), Fixed::from_f64(400.0));
        assert_eq!(wght.max_value(), Fixed::from_f64(900.0));
        assert_eq!(wght.flags(), 0);
        assert_eq!(wght.axis_name_id(), NameId::new(257));
    }

    #[test]
    fn instances() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let fvar = font.fvar().unwrap();
        assert_eq!(fvar.instance_count(), 9);
        // There are 9 instances equally spaced from 100.0 to 900.0
        // with name id monotonically increasing starting at 258.
        let instances = fvar.instances().unwrap();
        for i in 0..9 {
            let value = 100.0 * (i + 1) as f64;
            let name_id = NameId::new(258 + i as u16);
            let instance = instances.get(i).unwrap();
            assert_eq!(instance.coordinates.len(), 1);
            assert_eq!(
                instance.coordinates.get(0).unwrap().get(),
                Fixed::from_f64(value)
            );
            assert_eq!(instance.subfamily_name_id, name_id);
            assert_eq!(instance.post_script_name_id, None);
        }
    }

    #[test]
    fn normalize() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let fvar = font.fvar().unwrap();
        let axis = fvar.axes().unwrap().get(0).unwrap();
        let values = [100.0, 220.0, 250.0, 400.0, 650.0, 900.0];
        let expected = [-1.0, -0.60001, -0.5, 0.0, 0.5, 1.0];
        for (value, expected) in values.into_iter().zip(expected) {
            assert_eq!(
                axis.normalize(Fixed::from_f64(value)),
                Fixed::from_f64(expected)
            );
        }
    }
}
