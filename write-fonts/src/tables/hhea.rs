//! The hhea table

include!("../../generated/generated_hhea.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        let hhea = Hhea {
            ascender: FWord::new(800),
            descender: FWord::new(-200),
            line_gap: FWord::new(0),
            advance_width_max: UfWord::new(999),
            min_left_side_bearing: FWord::new(-50),
            min_right_side_bearing: FWord::new(-69),
            x_max_extent: FWord::new(888),
            caret_slope_rise: 3,
            caret_slope_run: 1,
            caret_offset: 0,
            number_of_h_metrics: 42,
        };

        let _dumped = crate::write::dump_table(&hhea).unwrap();
        #[cfg(feature = "parsing")]
        {
            let data = FontData::new(&_dumped);
            let loaded = read_fonts::tables::hhea::Hhea::read(data).unwrap();
            assert_eq!(loaded.advance_width_max(), hhea.advance_width_max);
            assert_eq!(loaded.ascender(), hhea.ascender);
            assert_eq!(loaded.descender(), hhea.descender);
            assert_eq!(loaded.version(), MajorMinor::VERSION_1_0);
            assert_eq!(loaded.min_left_side_bearing(), hhea.min_left_side_bearing);
            assert_eq!(loaded.min_right_side_bearing(), hhea.min_right_side_bearing);
            assert_eq!(loaded.min_right_side_bearing(), hhea.min_right_side_bearing);
            assert_eq!(loaded.number_of_h_metrics(), hhea.number_of_h_metrics);
        }
    }
}
