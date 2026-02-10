use skrifa::raw::ReadError;
use write_fonts::{read::tables::avar::Avar, types::F2Dot14};

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
