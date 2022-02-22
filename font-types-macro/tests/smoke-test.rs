use font_types::{BigEndian, F2Dot14};

font_types_macro::tables! {
    SegmentMaps<'a> {
        position_map_count: BigEndian<u16>,
        #[count(position_map_count)]
        axis_value_maps: [AxisValueMap],
    }

    AxisValueMap {
        from_coordinate: BigEndian<F2Dot14>,
        to_coordinate: BigEndian<F2Dot14>,
    }

    Avar<'a> {
        major_version: BigEndian<u16>,
        minor_version: BigEndian<u16>,
        #[hidden]
        reserved: BigEndian<u16>,
        axis_count: BigEndian<u16>,
        #[count(axis_count)]
        #[variable_size]
        axis_segment_maps: [SegmentMaps<'a>],
    }
}

impl<'a> font_types::VarSized<'a> for SegmentMaps<'a> {
    fn len(&self) -> usize {
        self.position_map_count() as usize * std::mem::size_of::<AxisValueMap>()
    }
}

fn div_by_two(arg: u16) -> usize {
    arg as usize / 2
}

fn main() {}
