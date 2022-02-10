pub struct AxisValueMap;

toy_table_macro::tables! {
    SegmentMaps<'a> {
        position_map_count: Uint16,
        #[count(position_map_count)]
        axis_value_maps: [AxisValueMap],
    }
}

fn main() {
}
