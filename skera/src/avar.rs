use skrifa::raw::{tables::avar::SegmentMaps, ReadError, TopLevelTable};
use write_fonts::{read::tables::avar::Avar, types::F2Dot14};

use crate::{
    serialize::SerializeErrorFlags,
    variations::solver::{renormalize_value, Triple},
    Subset, SubsetError,
};

pub(crate) fn map_coords_2_14(
    avar: &Avar,
    coords: Vec<F2Dot14>,
) -> Result<Vec<F2Dot14>, ReadError> {
    let maps = avar.axis_segment_maps();
    coords
        .into_iter()
        .zip(maps.iter())
        .map(|(coord, maybe_map)| maybe_map.map(|m| m.apply(coord.to_fixed()).to_f2dot14()))
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
                let unmapped_range: Triple = unmap_axis_range(axis_range, &segment_map);
                let mut value_mappings = vec![];
                for mapping in segment_map.axis_value_maps() {
                    let mapping_from = mapping.from_coordinate().to_f32();
                    if !unmapped_range.contains(mapping_from) {
                        continue;
                    }
                    let mapping_to = mapping.to_coordinate().to_f32();
                    let new_mapping = (
                        renormalize_value(mapping_from, unmapped_range, triple_distances, false),
                        renormalize_value(mapping_to, *axis_range, triple_distances, false),
                    );
                    if must_include(new_mapping) {
                        continue;
                    }
                    value_mappings.push(new_mapping);
                }
                value_mappings.push((-1.0, -1.0));
                value_mappings.push((0.0, 0.0));
                value_mappings.push((1.0, 1.0));
                value_mappings.sort_by_key(|(from, _)| F2Dot14::from_f32(*from));
                s.embed(value_mappings.len() as u16)?;
                for (from, to) in value_mappings {
                    s.embed(F2Dot14::from_f32(from))?;
                    s.embed(F2Dot14::from_f32(to))?;
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

fn unmap_float(f: f32, segment_maps: &SegmentMaps) -> f32 {
    map_float(f, Direction::Backward, segment_maps)
}

fn map_float(value: f32, direction: Direction, segment_maps: &SegmentMaps) -> f32 {
    let maps = segment_maps.axis_value_maps();
    let len = maps.len();
    if len < 2 {
        if len == 0 {
            return value;
        }
        return value - maps[0].from_coordinate().to_f32() + maps[0].to_coordinate().to_f32();
    }

    let from_coord = |index: usize| match direction {
        Direction::Forward => maps[index].from_coordinate().to_f32(),
        Direction::Backward => maps[index].to_coordinate().to_f32(),
    };
    let to_coord = |index: usize| match direction {
        Direction::Forward => maps[index].to_coordinate().to_f32(),
        Direction::Backward => maps[index].from_coordinate().to_f32(),
    };

    let mut start = 0usize;
    let mut end = len;
    if from_coord(start) == -1.0 && to_coord(start) == -1.0 && from_coord(start + 1) == -1.0 {
        start += 1;
    }
    if from_coord(end - 1) == 1.0 && to_coord(end - 1) == 1.0 && from_coord(end - 2) == 1.0 {
        end -= 1;
    }

    let mut i = start;
    while i < end {
        if value == from_coord(i) {
            break;
        }
        i += 1;
    }
    if i < end {
        let mut j = i;
        while j + 1 < end {
            if value != from_coord(j + 1) {
                break;
            }
            j += 1;
        }

        if i == j {
            return to_coord(i);
        }
        if i + 2 == j {
            return to_coord(i + 1);
        }

        if value < 0.0 {
            return to_coord(j);
        }
        if value > 0.0 {
            return to_coord(i);
        }

        return if to_coord(i).abs() < to_coord(j).abs() {
            to_coord(i)
        } else {
            to_coord(j)
        };
    }

    let mut i = start;
    while i < end {
        if value < from_coord(i) {
            break;
        }
        i += 1;
    }

    if i == 0 {
        return value - from_coord(0) + to_coord(0);
    }
    if i == end {
        return value - from_coord(end - 1) + to_coord(end - 1);
    }

    let before = i - 1;
    let after = i;
    let denom = from_coord(after) - from_coord(before);
    to_coord(before) + ((to_coord(after) - to_coord(before)) * (value - from_coord(before))) / denom
}

fn must_include(mapping: (f32, f32)) -> bool {
    mapping == (-1.0, -1.0) || mapping == (0.0, 0.0) || mapping == (1.0, 1.0)
}
