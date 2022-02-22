//! something to macro-expand when debugging

#![allow(dead_code)]

use font_types::{BigEndian, F2Dot14};

font_types_macro::tables! {
    /// Some segment maps
    ///
    /// These map segments
    SegmentMaps<'a> {
        /// Count of position maps
        position_map_count: BigEndian<u16>,
        /// maps of axisvalues
        #[count_with(identity, position_map_count)]
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

fn identity(t: u16) -> usize {
    t as _
}

impl<'a> font_types::VarSized<'a> for SegmentMaps<'a> {
    fn len(&self) -> usize {
        self.position_map_count() as usize * std::mem::size_of::<AxisValueMap>()
    }
}

fn main() {}
