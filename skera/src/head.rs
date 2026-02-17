//! impl subset() for head
use crate::{serialize::Serializer, Plan, Subset, SubsetError};
use write_fonts::{
    read::{tables::head::Head, FontRef, TopLevelTable},
    FontBuilder,
};

#[derive(Copy, Clone, Debug)]
pub(crate) struct HeadMaxpInfo {
    x_min: i16,
    x_max: i16,
    y_min: i16,
    y_max: i16,
    max_points: u16,
    max_contours: u16,
    max_composite_points: u16,
    max_composite_contours: u16,
    max_component_elements: u16,
    max_component_depth: u16,
    all_x_min_is_lsb: bool,
}
impl Default for HeadMaxpInfo {
    fn default() -> Self {
        Self {
            x_min: i16::MAX,
            x_max: i16::MIN,
            y_min: i16::MAX,
            y_max: i16::MIN,
            max_points: 0,
            max_contours: 0,
            max_composite_points: 0,
            max_composite_contours: 0,
            max_component_elements: 0,
            max_component_depth: 0,
            all_x_min_is_lsb: false,
        }
    }
}
impl HeadMaxpInfo {
    pub(crate) fn update_extrema(&mut self, x_min: i16, y_min: i16, x_max: i16, y_max: i16) {
        self.x_min = self.x_min.min(x_min);
        self.y_min = self.y_min.min(y_min);
        self.x_max = self.x_max.max(x_max);
        self.y_max = self.y_max.max(y_max);
    }
}

pub(crate) fn subset_head(head: &Head, loca_format: u8, plan: &Plan) -> Vec<u8> {
    let mut out = head.offset_data().as_bytes().to_owned();
    if !plan.normalized_coords.is_empty() {
        let x_min_start = head.shape().x_min_byte_range().start;
        let x_min = plan.head_maxp_info.borrow().x_min;

        out.get_mut(x_min_start..x_min_start + 2)
            .unwrap()
            .copy_from_slice(&x_min.to_be_bytes());

        let x_max_start = head.shape().x_max_byte_range().start;
        let x_max = plan.head_maxp_info.borrow().x_max;
        out.get_mut(x_max_start..x_max_start + 2)
            .unwrap()
            .copy_from_slice(&x_max.to_be_bytes());
        let y_min_start = head.shape().y_min_byte_range().start;
        let y_min = plan.head_maxp_info.borrow().y_min;
        out.get_mut(y_min_start..y_min_start + 2)
            .unwrap()
            .copy_from_slice(&y_min.to_be_bytes());
        let y_max_start = head.shape().y_max_byte_range().start;
        let y_max = plan.head_maxp_info.borrow().y_max;
        out.get_mut(y_max_start..y_max_start + 2)
            .unwrap()
            .copy_from_slice(&y_max.to_be_bytes());
    }
    out.get_mut(50..52)
        .unwrap()
        .copy_from_slice(&[0, loca_format]);
    out
}
