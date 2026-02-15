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

// reference: subset() for head in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-head-table.hh#L63
impl Subset for Head<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        s.embed_bytes(self.offset_data().as_bytes())
            .map_err(|_| SubsetError::SubsetTableError(Head::TAG))?;

        if !plan.normalized_coords.is_empty() {
            s.copy_assign(
                self.shape().x_min_byte_range().start,
                plan.head_maxp_info.borrow_mut().x_min,
            );
            s.copy_assign(
                self.shape().x_max_byte_range().start,
                plan.head_maxp_info.borrow_mut().x_max,
            );
            s.copy_assign(
                self.shape().y_min_byte_range().start,
                plan.head_maxp_info.borrow_mut().y_min,
            );
            s.copy_assign(
                self.shape().y_max_byte_range().start,
                plan.head_maxp_info.borrow_mut().y_max,
            );
        }
        Ok(())
    }
}
