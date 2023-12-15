//! The [avar](https://learn.microsoft.com/en-us/typography/opentype/spec/avar) table

include!("../../generated/generated_avar.rs");

impl SegmentMaps {
    /// Returns true if all the axis value maps are identity maps.
    pub fn is_identity(&self) -> bool {
        self.axis_value_maps
            .iter()
            .all(|av| av.from_coordinate == av.to_coordinate)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use font_types::F2Dot14;

    #[test]
    fn test_is_identity() {
        let mut segment_maps = SegmentMaps::default();

        assert!(segment_maps.is_identity());

        segment_maps.axis_value_maps.push(AxisValueMap {
            from_coordinate: F2Dot14::from_f32(-1.0),
            to_coordinate: F2Dot14::from_f32(-1.0),
        });

        assert!(segment_maps.is_identity());

        segment_maps.axis_value_maps.push(AxisValueMap {
            from_coordinate: F2Dot14::from_f32(0.0),
            to_coordinate: F2Dot14::from_f32(0.0),
        });
        segment_maps.axis_value_maps.push(AxisValueMap {
            from_coordinate: F2Dot14::from_f32(0.3),
            to_coordinate: F2Dot14::from_f32(0.6),
        });
        segment_maps.axis_value_maps.push(AxisValueMap {
            from_coordinate: F2Dot14::from_f32(1.0),
            to_coordinate: F2Dot14::from_f32(1.0),
        });

        assert!(!segment_maps.is_identity());
    }
}
