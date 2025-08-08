//! The [Axis Variations](https://docs.microsoft.com/en-us/typography/opentype/spec/avar) table

use super::variations::{DeltaSetIndexMap, ItemVariationStore};
use crate::FontRead;

include!("../../generated/generated_avar.rs");

impl SegmentMaps<'_> {
    /// Applies the piecewise linear mapping to the specified coordinate.
    pub fn apply(&self, coord: Fixed) -> Fixed {
        let maps = self.axis_value_maps();
        let coord_f32 = coord.to_f32();

        match maps.len() {
            0 => coord,
            1 => {
                let map = maps[0];
                coord - map.from_coordinate().to_fixed() + map.to_coordinate().to_fixed()
            }
            _ => {
                let mut start = 0;
                let mut end = maps.len();

                if maps.len() >= 2 {
                    if maps[0].from_coordinate().to_f32() == -1.0
                        && maps[0].to_coordinate().to_f32() == -1.0
                        && maps[1].from_coordinate().to_f32() == -1.0
                    {
                        start += 1;
                    }
                    if maps[end - 1].from_coordinate().to_f32() == 1.0
                        && maps[end - 1].to_coordinate().to_f32() == 1.0
                        && maps[end - 2].from_coordinate().to_f32() == 1.0
                    {
                        end -= 1;
                    }
                }

                let maps = &maps[start..end];

                // exact match
                let exact_matches: Vec<_> = maps
                    .iter()
                    .filter(|m| m.from_coordinate().to_f32() == coord_f32)
                    .collect();
                match exact_matches.len() {
                    0 => {} // fallthrough
                    1 => return exact_matches[0].to_coordinate().to_fixed(),
                    3 => return exact_matches[1].to_coordinate().to_fixed(),
                    _ => {
                        let (first, last) = (exact_matches[0], *exact_matches.last().unwrap());
                        return if coord_f32 < 0.0 {
                            last.to_coordinate().to_fixed()
                        } else if coord_f32 > 0.0 {
                            first.to_coordinate().to_fixed()
                        } else {
                            let f = first.to_coordinate().to_f32().abs();
                            let l = last.to_coordinate().to_f32().abs();
                            if f < l {
                                first.to_coordinate().to_fixed()
                            } else {
                                last.to_coordinate().to_fixed()
                            }
                        };
                    }
                }

                for i in 0..maps.len() {
                    let from = maps[i].from_coordinate().to_f32();
                    if coord_f32 < from {
                        if i == 0 {
                            let delta = coord - maps[0].from_coordinate().to_fixed();
                            return maps[0].to_coordinate().to_fixed() + delta;
                        }
                        let p = &maps[i - 1];
                        let n = &maps[i];
                        let p_from = p.from_coordinate().to_fixed();
                        let p_to = p.to_coordinate().to_fixed();
                        let n_from = n.from_coordinate().to_fixed();
                        let n_to = n.to_coordinate().to_fixed();
                        let delta = coord - p_from;
                        let scale = n_from - p_from;
                        return p_to + (n_to - p_to).mul_div(delta, scale);
                    }
                }

                let last = maps.last().unwrap();
                coord - last.from_coordinate().to_fixed() + last.to_coordinate().to_fixed()
            }
        }
    }
}

impl VarSize for SegmentMaps<'_> {
    type Size = u16;

    fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
        Some(
            data.read_at::<u16>(pos).ok()? as usize * AxisValueMap::RAW_BYTE_LEN
                + u16::RAW_BYTE_LEN,
        )
    }
}

impl<'a> FontRead<'a> for SegmentMaps<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        let position_map_count: BigEndian<u16> = cursor.read_be()?;
        let axis_value_maps = cursor.read_array(position_map_count.get() as _)?;
        Ok(SegmentMaps {
            position_map_count,
            axis_value_maps,
        })
    }
}

#[cfg(test)]
mod tests {
    use font_test_data::bebuffer::BeBuffer;
    use super::*;
    use crate::{FontRef, TableProvider};

    fn value_map(from: f32, to: f32) -> [F2Dot14; 2] {
        [F2Dot14::from_f32(from), F2Dot14::from_f32(to)]
    }

    impl PartialEq<[F2Dot14; 2]> for AxisValueMap {
        fn eq(&self, other: &[F2Dot14; 2]) -> bool {
            self.from_coordinate == other[0] && self.to_coordinate == other[1]
        }
    }

    #[test]
    fn segment_maps() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let avar = font.avar().unwrap();
        assert_eq!(avar.axis_count(), 1);
        let expected_segment_maps = &[vec![
            value_map(-1.0, -1.0),
            value_map(-0.6667, -0.5),
            value_map(-0.3333, -0.25),
            value_map(0.0, 0.0),
            value_map(0.2, 0.3674),
            value_map(0.4, 0.52246),
            value_map(0.6, 0.67755),
            value_map(0.8, 0.83875),
            value_map(1.0, 1.0),
        ]];
        let segment_maps = avar
            .axis_segment_maps()
            .iter()
            .map(|segment_map| segment_map.unwrap().axis_value_maps().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(segment_maps, expected_segment_maps);
    }

    #[test]
    fn segment_maps_multi_axis() {
        let segment_one_maps = [
            value_map(-1.0, -1.0),
            value_map(-0.6667, -0.5),
            value_map(-0.3333, -0.25),
        ];
        let segment_two_maps = [value_map(0.8, 0.83875), value_map(1.0, 1.0)];

        let data = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0u16)
            .push(2u16)
            .push(3u16)
            .extend(segment_one_maps[0])
            .extend(segment_one_maps[1])
            .extend(segment_one_maps[2])
            .push(2u16)
            .extend(segment_two_maps[0])
            .extend(segment_two_maps[1]);

        let avar = super::Avar::read(data.data().into()).unwrap();
        assert_eq!(avar.axis_segment_maps().iter().count(), 2);
        assert_eq!(avar.axis_segment_maps().get(0).unwrap().unwrap().axis_value_maps, segment_one_maps);
        assert_eq!(avar.axis_segment_maps().get(1).unwrap().unwrap().axis_value_maps, segment_two_maps);
    }

    #[test]
    fn piecewise_linear() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let avar = font.avar().unwrap();
        let segment_map = avar.axis_segment_maps().get(0).unwrap().unwrap();
        let coords = [-1.0, -0.5, 0.0, 0.5, 1.0];
        let expected_result = [-1.0, -0.375, 0.0, 0.6000061, 1.0];
        let results: Vec<f32> = coords
            .iter()
            .map(|c| segment_map.apply(Fixed::from_f64(*c as f64)).to_f32())
            .collect();
        for (res, exp) in results.iter().zip(expected_result.iter()) {
            assert!((res - exp).abs() < 0.0001);
        }
    }

    #[test]
    fn fallback_cases() {
        let single_map = SegmentMaps {
            position_map_count: 1.into(),
            axis_value_maps: &[AxisValueMap {
                from_coordinate: F2Dot14::from_f32(0.5).into(),
                to_coordinate: F2Dot14::from_f32(0.8).into(),
            }],
        };
        let coord = Fixed::from_f64(0.6);
        let expected = Fixed::from_f64(0.9);
        assert_eq!(single_map.apply(coord), expected);
    }
}

