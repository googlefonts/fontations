//! The Horizontal/Vertical Header tables.
//!
//! The [hhea](https://docs.microsoft.com/en-us/typography/opentype/spec/hhea)
//! and [vhea](https://docs.microsoft.com/en-us/typography/opentype/spec/hhea)
//! tables have the same structure and so we define them in the same module.

include!("../../generated/generated_hvhea.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        let hea = HVhea {
            ascender: FWord::new(800),
            descender: FWord::new(-200),
            line_gap: FWord::new(0),
            advance_max: UfWord::new(999),
            min_leading_bearing: FWord::new(-50),
            min_trailing_bearing: FWord::new(-69),
            max_extent: FWord::new(888),
            caret_slope_rise: 3,
            caret_slope_run: 1,
            caret_offset: 0,
            number_of_long_metrics: 42,
        };

        let _dumped = crate::write::dump_table(&hea).unwrap();
        #[cfg(feature = "parsing")]
        {
            let data = FontData::new(&_dumped);
            let loaded = read_fonts::tables::hvhea::HVhea::read(data).unwrap();
            assert_eq!(loaded.advance_max(), hea.advance_max);
            assert_eq!(loaded.ascender(), hea.ascender);
            assert_eq!(loaded.descender(), hea.descender);
            assert_eq!(loaded.version(), MajorMinor::VERSION_1_0);
            assert_eq!(loaded.min_leading_bearing(), hea.min_leading_bearing);
            assert_eq!(loaded.min_trailing_bearing(), hea.min_trailing_bearing);
            assert_eq!(loaded.min_trailing_bearing(), hea.min_trailing_bearing);
            assert_eq!(loaded.number_of_long_metrics(), hea.number_of_long_metrics);
        }
    }
}
