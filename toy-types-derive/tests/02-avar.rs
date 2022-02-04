use toy_types_derive::FontThing;
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

#[derive(FontThing)]
struct Avar<'a> {
    major_version: uint16,
    minor_version: uint16,
    reserved: uint16,
    axis_count: uint16,
    #[font_thing(count = "axis_count")]
    axis_segment_maps: VariableSizeArray<'a, SegmentMaps<'a>>,
}

impl<'a> DynamicSize<'a> for SegmentMaps<'a> {
    fn size(blob: Blob<'a>) -> Option<usize> {
        let size: u16 = blob.read(0)?;
        let item_size = std::mem::size_of::<AxisValueMap>() * size as usize;
        Some(item_size + u16::SIZE)
    }
}

fn main() {
}
