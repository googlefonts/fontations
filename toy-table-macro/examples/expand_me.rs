//! something to macro-expand when debugging

#![allow(dead_code)]

use raw_types::{F2Dot14, Uint16};

toy_table_macro::tables! {
    /// Some segment maps
    ///
    /// These map segments
    SegmentMaps<'a> {
        /// Count of position maps
        position_map_count: Uint16,
        /// maps of axisvalues
        #[count_with(identity, position_map_count)]
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

fn identity(t: raw_types::Uint16) -> usize {
    t.get() as _
}

impl<'a> raw_types::VarSized<'a> for SegmentMaps<'a> {
    fn len(&self) -> usize {
        self.position_map_count().get() as usize * std::mem::size_of::<AxisValueMap>()
    }
}

fn main() {}
