use font_types::{test_helpers, FontRead, BigEndian, F2Dot14};

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

//TODO: when we actually implement avar, tests should move to font-tables crate
fn main() {
    let mut buffer = test_helpers::BeBuffer::new();
    buffer.extend([1u16, 0u16]); // version
    buffer.push(69u16); // reserved
    buffer.push(1u16); // one axis
    // start segment maps:

    buffer.push(2u16); // position_map_count
    buffer.extend([F2Dot14::from_f32(1.0), F2Dot14::from_f32(-1.0)]);
    buffer.extend([F2Dot14::from_f32(1.75), F2Dot14::from_f32(-0.5)]);

    let avar = Avar::read(&buffer).unwrap();
    assert_eq!(avar.major_version(), 1);
}
