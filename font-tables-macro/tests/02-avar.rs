use font_types_macro::FontThing;
use toy_types::*;

#[derive(FontThing)]
struct AxisValueMap {
    from_coordinate: toy_types::F2dot14,
    to_coordinate: toy_types::F2dot14,
}

#[derive(FontThing)]
struct SegmentMaps<'a> {
    position_map_count: uint16,
    #[font_thing(count = "position_map_count")]
    axis_value_maps: Array<'a, AxisValueMap>,
}

//#[derive(FontThing)]
//struct Avar<'a> {
    //major_version: uint16,
    //minor_version: uint16,
    //reserved: uint16,
    //axis_count: uint16,
    //#[font_thing(count = "axis_count")]
    //axis_segment_maps: Array<'a, SegmentMaps<'a>>,
//}

fn main() {
}
