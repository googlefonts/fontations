struct AxisValueMap;
type uint16 = u16;

toy_table_macro::tables! {
    SegmentMaps {
        position_map_count: uint16,
        axis_value_maps: [AxisValueMap],
    }
}

toy_table_macro::tables! {
    #[explode(true)]
    OtherThing {
        position_map_count: uint16,
        #[count(position_map_count)]
        axis_value_maps: [AxisValueMap],
    }
}

fn main() {
}
