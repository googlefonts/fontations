//! The [kern](https://docs.microsoft.com/en-us/typography/opentype/spec/kern) table

include!("../../generated/generated_kern.rs");

impl Kern0 {
    fn compute_length(&self) -> u16 {
        const KERN_PAIR_LEN: usize = 6;
        let len = u16::RAW_BYTE_LEN * 7 + // format, length, coverage, num_pairs,
                                          // search_range, entry_selector, range_shift
        self.kerning_pairs.len() * KERN_PAIR_LEN;
        u16::try_from(len).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::util::SearchRange;

    use super::*;

    #[test]
    fn smoke_test() {
        let pairs = vec![
            KernPair::new(4, 12, FWord::new(-40)),
            KernPair::new(4, 28, FWord::new(40)),
            KernPair::new(5, 40, FWord::new(-50)),
        ];
        //searchRange, entrySelector, rangeShift = getSearchRange(pairs.len(), 6);
        let computed = SearchRange::compute(pairs.len(), 6);
        let kern0 = Kern0::new(
            KernCoverage::HORIZONTAL,
            computed.search_range,
            computed.entry_selector,
            computed.range_shift,
            pairs,
        );

        let kern = Kern::new(vec![kern0]);

        let bytes = crate::dump_table(&kern).unwrap();
        assert_eq!(bytes, font_test_data::kern::KERN_VER_0_FMT_0_DATA);
    }
}
