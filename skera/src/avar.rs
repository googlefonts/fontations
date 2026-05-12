use font_types::Fixed;
use skrifa::raw::{tables::avar::SegmentMaps, ReadError};
use write_fonts::read::tables::avar::Avar;

use crate::variations::solver::Triple;

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

fn unmap_axis_range(range: &Triple, segment_maps: &SegmentMaps) -> Triple {
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

fn unmap_float(f: f64, segment_maps: &SegmentMaps) -> f64 {
    map_float(f, Direction::Backward, segment_maps)
}

fn map_float(value: f64, direction: Direction, segment_maps: &SegmentMaps) -> f64 {
    let maps = segment_maps.axis_value_maps();
    let len = maps.len();
    if len < 2 {
        if len == 0 {
            return value;
        }
        let from_coord = maps[0].from_coordinate().to_f32() as f64;
        let to_coord = maps[0].to_coordinate().to_f32() as f64;
        return value - from_coord + to_coord;
    }

    let get_from_coord_val = |index: usize| match direction {
        Direction::Forward => maps[index].from_coordinate().to_bits() as f64 / 16384.0,
        Direction::Backward => maps[index].to_coordinate().to_bits() as f64 / 16384.0,
    };
    let get_to_coord_val = |index: usize| match direction {
        Direction::Forward => maps[index].to_coordinate().to_bits() as f64 / 16384.0,
        Direction::Backward => maps[index].from_coordinate().to_bits() as f64 / 16384.0,
    };

    let mut start = 0usize;
    let mut end = len;
    if get_from_coord_val(start) == -1.0
        && get_to_coord_val(start) == -1.0
        && get_from_coord_val(start + 1) == -1.0
    {
        start += 1;
    }
    if get_from_coord_val(end - 1) == 1.0
        && get_to_coord_val(end - 1) == 1.0
        && get_from_coord_val(end - 2) == 1.0
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

        if value < 0.0 {
            return get_to_coord_val(j);
        }
        if value > 0.0 {
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

fn float_approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < F_EPSILON
}

fn must_include(mapping: (f64, f64)) -> bool {
    // Only check for f64, as this is where the `new_mapping` values come from
    let neg_one = -1.0;
    let zero = 0.0;
    let one = 1.0;

    let map_from = mapping.0;
    let map_to = mapping.1;

    (float_approx_eq(map_from, neg_one) && float_approx_eq(map_to, neg_one))
        || (float_approx_eq(map_from, zero) && float_approx_eq(map_to, zero))
        || (float_approx_eq(map_from, one) && float_approx_eq(map_to, one))
}
