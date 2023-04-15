//! The [avar](https://learn.microsoft.com/en-us/typography/opentype/spec/avar) table

include!("../../generated/generated_avar.rs");


#[cfg(test)]
mod tests {
    use font_types::MajorMinor;
    use read_fonts::{FontRead, FontData};
    use crate::tables::avar::{Avar, SegmentMaps};

    use crate::dump_table;

    #[test]
    fn hangs() {
        let avar = Avar::new(
            MajorMinor::VERSION_1_0,
            vec![SegmentMaps::new(Vec::new())]);
        let bytes = dump_table(&avar).unwrap();
        Avar::read(FontData::new(&bytes)).unwrap();
    }
}