toy_table_macro::tables! {
    SegmentMaps<'a> {
        position_map_count: Uint16,
        #[count(position_map_count)]
        axis_value_maps: [AxisValueMap],
    }

    AxisValueMap {
        from_coordinate: F2Dot14,
        to_coordinate: F2Dot14,
    }

    Avar<'a> {
        major_version: Uint16,
        minor_version: Uint16,
        #[hidden]
        reserved: Uint16,
        axis_count: Uint16,
        #[count(axis_count)]
        #[variable_size]
        axis_segment_maps: [SegmentMaps<'a>],
    }
}

fn div_by_two(arg: raw_types::Uint16) -> usize {
    arg.get() as usize / 2
}

fn main() {}
