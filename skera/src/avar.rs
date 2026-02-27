use font_types::Fixed;
use num_traits::Float;
use skrifa::raw::{tables::avar::SegmentMaps, ReadError, TopLevelTable};
use write_fonts::{read::tables::avar::Avar, types::F2Dot14};

use crate::{
    serialize::SerializeErrorFlags,
    variations::solver::{renormalize_value, Triple, TripleDistances},
    Subset, SubsetError,
};

pub(crate) fn map_coords_2_14(avar: &Avar, coords: Vec<f32>) -> Result<Vec<f32>, ReadError> {
    let maps = avar.axis_segment_maps();
    coords
        .into_iter()
        .zip(maps.iter())
        .map(|(coord, maybe_map)| {
            maybe_map.map(|m| m.apply(Fixed::from_f64(coord as f64)).to_f32())
        })
        .collect()
}

impl Subset for Avar<'_> {
    fn subset(
        &self,
        plan: &crate::Plan,
        _font: &write_fonts::read::FontRef,
        s: &mut crate::serialize::Serializer,
        _builder: &mut write_fonts::FontBuilder,
    ) -> Result<(), crate::SubsetError> {
        if plan.axes_index_map.is_empty() {
            return Err(SubsetError::SubsetTableError(Avar::TAG)); // empty
        }
        subset_avar(self, plan, s).map_err(|_| SubsetError::SubsetTableError(Avar::TAG))
    }
}

fn subset_avar(
    avar: &Avar<'_>,
    plan: &crate::Plan,
    s: &mut crate::serialize::Serializer,
) -> Result<(), SerializeErrorFlags> {
    let new_axis_count = plan.axes_index_map.len() as u16;

    // Version
    s.embed(1_u16)?;
    s.embed(0_u16)?;
    s.embed(0_u16)?; // reserved
    s.embed(new_axis_count)?;

    for (i, segment_map) in avar.axis_segment_maps().iter().enumerate() {
        let Ok(segment_map) = segment_map else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
        };
        if plan.axes_index_map.contains_key(&i) {
            let Some(axis_tag) = plan.axes_old_index_tag_map.get(&i) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            // Subset the mapping
            if let Some(axis_range) = plan.axes_location.get(axis_tag) {
                let Some(&triple_distances) = plan.axes_triple_distances.get(axis_tag) else {
                    continue;
                };
                let unmapped_range: Triple<f64> = unmap_axis_range(axis_range, &segment_map);
                let axis_range =
                    Triple::new(axis_range.minimum, axis_range.middle, axis_range.maximum);
                let triple_distances =
                    TripleDistances::new(triple_distances.negative, triple_distances.positive);
                let mut value_mappings = vec![];
                for mapping in segment_map.axis_value_maps() {
                    let mapping_from = mapping.from_coordinate().to_f32() as f64;
                    if !unmapped_range.contains(mapping_from) {
                        continue;
                    }
                    let mapping_to = mapping.to_coordinate().to_f32() as f64;
                    let new_mapping = (
                        renormalize_value(
                            mapping_from,
                            unmapped_range,
                            TripleDistances::from(unmapped_range),
                            false,
                        ),
                        renormalize_value(mapping_to, axis_range, triple_distances, false),
                    );

                    if must_include(new_mapping) {
                        continue;
                    }
                    value_mappings.push(new_mapping);
                }
                value_mappings.push((-1.0, -1.0));
                value_mappings.push((0.0, 0.0));
                value_mappings.push((1.0, 1.0));
                value_mappings.sort_by_key(|(from, _)| F2Dot14::from_f32(*from as f32).to_bits());
                s.embed(value_mappings.len() as u16)?;
                for (from, to) in value_mappings {
                    s.embed(F2Dot14::from_f32(from as f32))?;
                    s.embed(F2Dot14::from_f32(to as f32))?;
                }
            } else {
                // Just embed it as-is
                s.embed(segment_map.position_map_count())?;
                for mapping in segment_map.axis_value_maps() {
                    s.embed(mapping.from_coordinate())?;
                    s.embed(mapping.to_coordinate())?;
                }
            }
        }
    }
    Ok(())
}

fn unmap_axis_range(range: &Triple<f64>, segment_maps: &SegmentMaps) -> Triple<f64> {
    Triple {
        minimum: unmap_float(range.minimum, segment_maps),
        middle: unmap_float(range.middle, segment_maps),
        maximum: unmap_float(range.maximum, segment_maps),
    }
}

enum Direction {
    #[allow(dead_code)]
    Forward,
    Backward,
}

fn unmap_float<F: Float + std::fmt::Debug + Copy + Default + PartialEq>(
    f: F,
    segment_maps: &SegmentMaps,
) -> F {
    map_float(f, Direction::Backward, segment_maps)
}

fn map_float<F: Float + std::fmt::Debug + Copy + Default + PartialEq>(
    value: F,
    direction: Direction,
    segment_maps: &SegmentMaps,
) -> F {
    let maps = segment_maps.axis_value_maps();
    let len = maps.len();
    if len < 2 {
        if len == 0 {
            return value;
        }
        let from_coord = F::from(maps[0].from_coordinate().to_bits() as f64 / 16384.0).unwrap();
        let to_coord = F::from(maps[0].to_coordinate().to_bits() as f64 / 16384.0).unwrap();
        return value - from_coord + to_coord;
    }

    let get_from_coord_val = |index: usize| match direction {
        Direction::Forward => {
            F::from(maps[index].from_coordinate().to_bits() as f64 / 16384.0).unwrap()
        }
        Direction::Backward => {
            F::from(maps[index].to_coordinate().to_bits() as f64 / 16384.0).unwrap()
        }
    };
    let get_to_coord_val = |index: usize| match direction {
        Direction::Forward => {
            F::from(maps[index].to_coordinate().to_bits() as f64 / 16384.0).unwrap()
        }
        Direction::Backward => {
            F::from(maps[index].from_coordinate().to_bits() as f64 / 16384.0).unwrap()
        }
    };

    let mut start = 0usize;
    let mut end = len;
    if get_from_coord_val(start) == -F::one()
        && get_to_coord_val(start) == -F::one()
        && get_from_coord_val(start + 1) == -F::one()
    {
        start += 1;
    }
    if get_from_coord_val(end - 1) == F::one()
        && get_to_coord_val(end - 1) == F::one()
        && get_from_coord_val(end - 2) == F::one()
    {
        end -= 1;
    }

    let mut i = start;
    while i < end {
        if value == get_from_coord_val(i) {
            break;
        }
        i += 1;
    }
    if i < end {
        let mut j = i;
        while j + 1 < end {
            if value != get_from_coord_val(j + 1) {
                break;
            }
            j += 1;
        }

        if i == j {
            return get_to_coord_val(i);
        }
        if i + 2 == j {
            return get_to_coord_val(i + 1);
        }

        if value < F::zero() {
            return get_to_coord_val(j);
        }
        if value > F::zero() {
            return get_to_coord_val(i);
        }

        return if get_to_coord_val(i).abs() < get_to_coord_val(j).abs() {
            get_to_coord_val(i)
        } else {
            get_to_coord_val(j)
        };
    }

    let mut i = start;
    while i < end {
        if value < get_from_coord_val(i) {
            break;
        }
        i += 1;
    }

    if i == 0 {
        return value - get_from_coord_val(0) + get_to_coord_val(0);
    }
    if i == end {
        return value - get_from_coord_val(end - 1) + get_to_coord_val(end - 1);
    }

    let before = i - 1;
    let after = i;
    let denom = get_from_coord_val(after) - get_from_coord_val(before);
    get_to_coord_val(before)
        + ((get_to_coord_val(after) - get_to_coord_val(before))
            * (value - get_from_coord_val(before)))
            / denom
}

const F_EPSILON: f64 = 0.00001; // Epsilon for float comparison

fn float_approx_eq<F: Float + std::fmt::Debug + Copy + Default + PartialEq>(a: F, b: F) -> bool {
    (a - b).abs() < F::from(F_EPSILON).unwrap()
}

fn must_include<F: Float + std::fmt::Debug + Copy + Default + PartialEq>(mapping: (F, F)) -> bool {
    // Only check for f64, as this is where the `new_mapping` values come from
    let neg_one = F::from(-1.0).unwrap();
    let zero = F::zero();
    let one = F::one();

    let map_from = mapping.0;
    let map_to = mapping.1;

    (float_approx_eq(map_from, neg_one) && float_approx_eq(map_to, neg_one))
        || (float_approx_eq(map_from, zero) && float_approx_eq(map_to, zero))
        || (float_approx_eq(map_from, one) && float_approx_eq(map_to, one))
}
