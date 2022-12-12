//! The [os2](https://docs.microsoft.com/en-us/typography/opentype/spec/os2) table

include!("../../generated/generated_os2.rs");

impl Os2 {
    fn compute_version(&self) -> u16 {
        if self.us_lower_optical_point_size.is_some() || self.us_upper_optical_point_size.is_some()
        {
            5
        } else if self.sx_height.or(self.s_cap_height).is_some()
            || self
                .us_default_char
                .or(self.us_break_char)
                .or(self.us_max_context)
                .is_some()
        {
            2
        } else {
            u16::from(
                self.ul_code_page_range_1
                    .or(self.ul_code_page_range_2)
                    .is_some(),
            )
        }
    }
}

fn convert_panose(raw: &[u8]) -> [u8; 10] {
    raw.try_into().unwrap_or_default()
}
