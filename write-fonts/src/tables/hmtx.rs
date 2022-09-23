//! The hmtx table

include!("../../generated/generated_hmtx.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        let hmtx = Hmtx {
            h_metrics: vec![LongHorMetric {
                advance_width: 602,
                lsb: -214,
            }],
            left_side_bearings: vec![-20, -32, -44, -6],
        };

        let _dumped = crate::write::dump_table(&hmtx).unwrap();

        #[cfg(feature = "parsing")]
        {
            let data = FontData::new(&_dumped);
            let loaded = read_fonts::tables::hmtx::Hmtx::read_with_args(data, &(1, 5)).unwrap();
            assert_eq!(loaded.h_metrics()[0].advance_width(), 602);
            assert_eq!(loaded.h_metrics()[0].lsb(), -214);
            assert_eq!(loaded.left_side_bearings(), &hmtx.left_side_bearings);
        }
    }
}
