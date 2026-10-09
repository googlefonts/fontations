//! the [vhea (Horizontal Header)](https://docs.microsoft.com/en-us/typography/opentype/spec/hhea) table

include!("../../generated/generated_vhea.rs");

/// A vertical header from VHEA or vhea.
#[derive(Clone)]
pub enum VheaTable<'a> {
    Standard(Vhea<'a>),
    Extended(VheaExtended<'a>),
}

impl VheaTable<'_> {
    /// Number of long metric records.
    pub fn number_of_long_ver_metrics(&self) -> u32 {
        match self {
            Self::Standard(table) => table.number_of_long_ver_metrics().into(),
            Self::Extended(table) => table.number_of_long_ver_metrics(),
        }
    }

    /// Maximum advance height, preserving VHEA's signed field.
    pub fn advance_height_max(&self) -> i32 {
        match self {
            Self::Standard(table) => table.advance_height_max().to_u16() as i32,
            Self::Extended(table) => table.advance_height_max().to_i16() as i32,
        }
    }

    extended_table_getters!(
        version -> Version16Dot16,
        ascender -> FWord,
        descender -> FWord,
        line_gap -> FWord,
        min_top_side_bearing -> FWord,
        min_bottom_side_bearing -> FWord,
        y_max_extent -> FWord,
        caret_slope_rise -> i16,
        caret_slope_run -> i16,
        caret_offset -> i16,
        metric_data_format -> i16,
    );
}
