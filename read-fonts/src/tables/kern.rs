//! The [kern](https://docs.microsoft.com/en-us/typography/opentype/spec/kern) table

use crate::{FontRead, VarSize};

include!("../../generated/generated_kern.rs");

impl VarSize for Kern0<'_> {
    type Size = u16;

    fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
        let length_offset = pos + std::mem::size_of::<u16>();
        data.read_at::<u16>(length_offset).ok().map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use crate::FontData;

    use super::*;
    fn kern_pair(left: u16, right: u16, value: i16) -> KernPair {
        KernPair {
            left: left.into(),
            right: right.into(),
            value: FWord::new(value).into(),
        }
    }

    #[test]
    fn smoke_test() {
        let data = FontData::new(font_test_data::kern::KERN_VER_0_FMT_0_DATA);
        let kern = Kern::read(data).unwrap();
        assert_eq!(kern.version(), 0);
        assert_eq!(kern.num_tables(), 1);

        let subtable = kern.subtables().iter().next().unwrap().unwrap();
        assert_eq!(subtable.format(), 0);
        assert_eq!(subtable.length(), 32);
        assert_eq!(subtable.coverage(), KernCoverage::HORIZONTAL);
        assert_eq!(subtable.num_pairs(), 3);
        assert_eq!(subtable.search_range(), 12);
        assert_eq!(subtable.entry_selector(), 1);
        assert_eq!(subtable.range_shift(), 6);

        let pairs = subtable.kerning_pairs();
        assert_eq!(
            pairs,
            [
                kern_pair(4, 12, -40),
                kern_pair(4, 28, 40),
                kern_pair(5, 40, -50),
            ]
        )
    }
}
