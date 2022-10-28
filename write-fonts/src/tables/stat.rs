//! The STAT table

include!("../../generated/generated_stat.rs");

// we use a custom conversion here because we use a shim table in read-fonts
// (required because it is an offset to an array of offsets, which is too recursive for us)
// but in write-fonts we want to skip the shim table and just use a vec.
fn convert_axis_value_offsets(
    from: Result<read_fonts::tables::stat::AxisValueArray, ReadError>,
) -> OffsetMarker<Vec<OffsetMarker<AxisValue>>, WIDTH_32> {
    OffsetMarker::new_maybe_null(from.ok().map(|array| {
        array
            .axis_values()
            .map(|val| val.to_owned_obj(array.offset_data()))
            .collect()
    }))
}
