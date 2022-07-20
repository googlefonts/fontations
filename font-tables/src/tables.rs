//! Font tables.

pub mod cmap;
pub mod gdef;
pub mod glyf;
pub mod gpos;
pub mod head;
pub mod hhea;
pub mod hmtx;
pub mod loca;
pub mod maxp;
pub mod name;
pub mod post;
pub mod stat;

use font_types::{FontRead, FontReadWithArgs, Tag};

/// An interface for accessing tables from a font (or font-like object)
pub trait TableProvider {
    fn data_for_tag(&self, tag: Tag) -> Option<&[u8]>;

    fn head(&self) -> Option<head::Head> {
        self.data_for_tag(head::TAG).and_then(head::Head::read)
    }

    fn name(&self) -> Option<name::Name> {
        self.data_for_tag(name::TAG).and_then(name::Name::read)
    }

    fn hhea(&self) -> Option<hhea::Hhea> {
        self.data_for_tag(hhea::TAG).and_then(hhea::Hhea::read)
    }

    fn hmtx(&self) -> Option<hmtx::Hmtx> {
        //FIXME: should we make the user pass these in?
        let num_glyphs = self.maxp().map(|maxp| maxp.num_glyphs())?;
        let number_of_h_metrics = self.hhea().map(|hhea| hhea.number_of_h_metrics())?;
        self.data_for_tag(hmtx::TAG)
            .and_then(|data| hmtx::Hmtx::read_with_args(data, &(num_glyphs, number_of_h_metrics)))
            .map(|(table, _)| table)
    }

    fn maxp(&self) -> Option<maxp::Maxp> {
        self.data_for_tag(maxp::TAG).and_then(maxp::Maxp::read)
    }

    fn post(&self) -> Option<post::Post> {
        self.data_for_tag(post::TAG).and_then(post::Post::read)
    }

    fn stat(&self) -> Option<stat::Stat> {
        self.data_for_tag(stat::TAG).and_then(stat::Stat::read)
    }

    fn loca(&self, num_glyphs: u16, is_long: bool) -> Option<loca::Loca> {
        let bytes = self.data_for_tag(loca::TAG)?;
        loca::Loca::read(bytes, num_glyphs, is_long)
    }

    fn glyf(&self) -> Option<glyf::Glyf> {
        self.data_for_tag(glyf::TAG).and_then(glyf::Glyf::read)
    }

    fn cmap(&self) -> Option<cmap::Cmap> {
        self.data_for_tag(cmap::TAG).and_then(cmap::Cmap::read)
    }

    fn gdef(&self) -> Option<gdef::Gdef> {
        self.data_for_tag(gdef::TAG).and_then(gdef::Gdef::read)
    }

    fn gpos(&self) -> Option<gpos::Gpos> {
        self.data_for_tag(gpos::TAG).and_then(FontRead::read)
    }
}

pub mod test_gpos2 {
    use super::gpos::ValueRecord;
    use super::layout2::CoverageTable;

    impl ValueFormat {
        /// Return the number of bytes required to store a [`ValueRecord`] in this format.
        #[inline]
        pub fn record_byte_len(self) -> usize {
            self.bits().count_ones() as usize * u16::RAW_BYTE_LEN
        }
    }

    fn class1_record_len(
        class1_count: u16,
        class2_count: u16,
        format1: ValueFormat,
        format2: ValueFormat,
    ) -> usize {
        (format1.record_byte_len() + format2.record_byte_len())
            * class1_count as usize
            * class2_count as usize
    }

    impl<'a> SinglePosFormat1<'a> {
        pub fn value_record(&self) -> ValueRecord {
            self.data
                .read_at_with(self.shape.value_record_byte_range().start, |bytes| {
                    ValueRecord::read2(bytes, self.value_format()).ok_or(ReadError::OutOfBounds)
                })
                .unwrap_or_default()
        }
    }

    impl<'a> SinglePosFormat2<'a> {
        pub fn value_records(&self) -> impl Iterator<Item = ValueRecord> + '_ {
            let count = self.value_count() as usize;
            let format = self.value_format();

            (0..count).map(move |idx| {
                let offset =
                    self.shape.value_records_byte_range().start + (idx * format.record_byte_len());
                self.data
                    .read_at_with(offset, |bytes| {
                        ValueRecord::read2(bytes, format).ok_or(ReadError::OutOfBounds)
                    })
                    .unwrap_or_default()
            })
        }
    }

    include!("../generated/gpos2.rs");
}

pub mod layout2 {

    include!("../generated/layout2.rs");
    fn delta_value_count(start_size: u16, end_size: u16, delta_format: DeltaFormat) -> usize {
        let range_len = start_size.saturating_add(1).saturating_sub(end_size) as usize;
        let val_per_word = match delta_format {
            DeltaFormat::Local2BitDeltas => 8,
            DeltaFormat::Local4BitDeltas => 4,
            DeltaFormat::Local8BitDeltas => 2,
            _ => return 0,
        };

        let count = range_len / val_per_word;
        let extra = (range_len % val_per_word).min(1);
        count + extra
    }

    #[cfg(test)]
    mod tests {
        use font_types::OffsetHost;

        use super::*;
        fn try_read<'a, T: super::FontRead<'a>>(bytes: &'a [u8]) -> Result<T, super::ReadError> {
            let data = super::FontData::new(bytes);
            T::read(data)
        }

        #[test]
        fn example_1_scripts() {
            // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-1-scriptlist-table-and-scriptrecords
            #[rustfmt::skip]
        let bytes = [
            0x00, 0x03, 0x68, 0x61, 0x6E, 0x69, 0x00, 0x14, 0x6B, 0x61, 0x6E,
            0x61, 0x00, 0x18, 0x6C, 0x61, 0x74, 0x6E, 0x00, 0x1C,
        ];

            let table: ScriptList = try_read(&bytes).unwrap();
            assert_eq!(table.script_count(), 3);
            let first = &table.script_records()[0];
            assert_eq!(first.script_tag.get(), Tag::new(b"hani"));
            assert_eq!(first.script_offset.get(), 0x14);
        }

        #[test]
        fn example_2_scripts_and_langs() {
            // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-2-script-table-langsysrecord-and-langsys-table
            #[rustfmt::skip]
        let bytes = [
            0x00, 0x0A, 0x00, 0x01, 0x55, 0x52, 0x44, 0x20, 0x00, 0x16, 0x00,
            0x00, 0xFF, 0xFF, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02,
            0x00, 0x00, 0x00, 0x03, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x02,
        ];

            let table: Script = try_read(&bytes).unwrap();
            let def_lang = table.default_lang_sys().unwrap().unwrap();
            assert_eq!(def_lang.required_feature_index(), 0xFFFF);
            assert_eq!(def_lang.feature_index_count(), 3);
            assert_eq!(def_lang.feature_indices(), [0u16, 1, 2]);
        }

        #[test]
        fn example_3_featurelist_and_feature() {
            // https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#example-3-featurelist-table-and-feature-table
            #[rustfmt::skip]
        let bytes = [
            0x00, 0x03, 0x6C, 0x69, 0x67, 0x61, 0x00, 0x14, 0x6C, 0x69, 0x67,
            0x61, 0x00, 0x1A, 0x6C, 0x69, 0x67, 0x61, 0x00, 0x22, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02,
        ];

            let table: FeatureList = try_read(&bytes).unwrap();
            assert_eq!(table.feature_count(), 3);
            let record1 = &table.feature_records()[0];
            let turkish_liga: Feature = table.resolve_offset(record1.feature_offset.get()).unwrap();
            assert_eq!(turkish_liga.lookup_index_count(), 1);
        }
    }
}
