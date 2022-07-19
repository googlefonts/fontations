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
