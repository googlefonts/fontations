//! a trait for things that can serve font tables

use types::Tag;

use crate::{tables, FontData, FontRead, FontReadWithArgs, ReadError};

/// An interface for accessing tables from a font (or font-like object)
pub trait TableProvider<'a> {
    fn data_for_tag(&self, tag: Tag) -> Option<FontData<'a>>;

    fn expect_data_for_tag(&self, tag: Tag) -> Result<FontData<'a>, ReadError> {
        self.data_for_tag(tag).ok_or(ReadError::TableIsMissing(tag))
    }

    fn head(&self) -> Result<tables::head::Head<'a>, ReadError> {
        self.expect_data_for_tag(tables::head::TAG)
            .and_then(FontRead::read)
    }

    fn name(&self) -> Result<tables::name::Name<'a>, ReadError> {
        self.expect_data_for_tag(tables::name::TAG)
            .and_then(FontRead::read)
    }

    fn hhea(&self) -> Result<tables::hvhea::HVhea<'a>, ReadError> {
        self.expect_data_for_tag(tables::hhea::TAG)
            .and_then(FontRead::read)
    }

    fn vhea(&self) -> Result<tables::hvhea::HVhea<'a>, ReadError> {
        self.expect_data_for_tag(tables::vhea::TAG)
            .and_then(FontRead::read)
    }

    fn hmtx(&self) -> Result<tables::hvmtx::HVmtx<'a>, ReadError> {
        //FIXME: should we make the user pass these in?
        let num_glyphs = self.maxp().map(|maxp| maxp.num_glyphs())?;
        let number_of_h_metrics = self.hhea().map(|hhea| hhea.number_of_long_metrics())?;
        self.expect_data_for_tag(tables::hmtx::TAG)
            .and_then(|data| {
                FontReadWithArgs::read_with_args(data, &(number_of_h_metrics, num_glyphs))
            })
    }

    fn vmtx(&self) -> Result<tables::hvmtx::HVmtx<'a>, ReadError> {
        //FIXME: should we make the user pass these in?
        let num_glyphs = self.maxp().map(|maxp| maxp.num_glyphs())?;
        let number_of_v_metrics = self.vhea().map(|vhea| vhea.number_of_long_metrics())?;
        self.expect_data_for_tag(tables::vmtx::TAG)
            .and_then(|data| {
                FontReadWithArgs::read_with_args(data, &(number_of_v_metrics, num_glyphs))
            })
    }

    fn avar(&self) -> Result<tables::avar::Avar<'a>, ReadError> {
        self.expect_data_for_tag(tables::avar::TAG)
            .and_then(FontRead::read)
    }

    fn hvar(&self) -> Result<tables::hvar::Hvar<'a>, ReadError> {
        self.expect_data_for_tag(tables::hvar::TAG)
            .and_then(FontRead::read)
    }

    fn vvar(&self) -> Result<tables::vvar::Vvar<'a>, ReadError> {
        self.expect_data_for_tag(tables::vvar::TAG)
            .and_then(FontRead::read)
    }

    fn mvar(&self) -> Result<tables::mvar::Mvar<'a>, ReadError> {
        self.expect_data_for_tag(tables::mvar::TAG)
            .and_then(FontRead::read)
    }

    fn maxp(&self) -> Result<tables::maxp::Maxp<'a>, ReadError> {
        self.expect_data_for_tag(tables::maxp::TAG)
            .and_then(FontRead::read)
    }

    fn os2(&self) -> Result<tables::os2::Os2<'a>, ReadError> {
        self.expect_data_for_tag(tables::os2::TAG)
            .and_then(FontRead::read)
    }

    fn post(&self) -> Result<tables::post::Post<'a>, ReadError> {
        self.expect_data_for_tag(tables::post::TAG)
            .and_then(FontRead::read)
    }

    //fn stat(&self) -> Option<stat::Stat> {
    //self.data_for_tag(stat::TAG).and_then(stat::Stat::read)
    //}

    /// is_long can be optionally provided, if known, otherwise we look it up in head.
    fn loca(&self, is_long: impl Into<Option<bool>>) -> Result<tables::loca::Loca<'a>, ReadError> {
        let is_long = match is_long.into() {
            Some(val) => val,
            None => self.head()?.index_to_loc_format() == 1,
        };
        self.expect_data_for_tag(tables::loca::TAG)
            .and_then(|data| FontReadWithArgs::read_with_args(data, &is_long))
    }

    fn glyf(&self) -> Result<tables::glyf::Glyf<'a>, ReadError> {
        self.expect_data_for_tag(tables::glyf::TAG)
            .and_then(FontRead::read)
    }

    fn cmap(&self) -> Result<tables::cmap::Cmap<'a>, ReadError> {
        self.expect_data_for_tag(tables::cmap::TAG)
            .and_then(FontRead::read)
    }

    fn gdef(&self) -> Result<tables::gdef::Gdef<'a>, ReadError> {
        self.expect_data_for_tag(tables::gdef::TAG)
            .and_then(FontRead::read)
    }

    fn gpos(&self) -> Result<tables::gpos::Gpos<'a>, ReadError> {
        self.expect_data_for_tag(tables::gpos::TAG)
            .and_then(FontRead::read)
    }

    fn gsub(&self) -> Result<tables::gsub::Gsub<'a>, ReadError> {
        self.expect_data_for_tag(tables::gsub::TAG)
            .and_then(FontRead::read)
    }

    fn colr(&self) -> Result<tables::colr::Colr<'a>, ReadError> {
        self.expect_data_for_tag(tables::colr::TAG)
            .and_then(FontRead::read)
    }

    fn cpal(&self) -> Result<tables::cpal::Cpal<'a>, ReadError> {
        self.expect_data_for_tag(tables::cpal::TAG)
            .and_then(FontRead::read)
    }

    fn stat(&self) -> Result<tables::stat::Stat<'a>, ReadError> {
        self.expect_data_for_tag(tables::stat::TAG)
            .and_then(FontRead::read)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /// https://github.com/googlefonts/fontations/issues/105
    #[test]
    fn bug_105() {
        // serve some dummy versions of the tables used to compute hmtx. The only
        // fields that matter are maxp::num_glyphs and hhea::number_of_h_metrics,
        // everything else is zero'd out
        struct DummyProvider;
        impl TableProvider<'static> for DummyProvider {
            fn data_for_tag(&self, tag: Tag) -> Option<FontData<'static>> {
                if tag == Tag::new(b"maxp") {
                    Some(FontData::new(&[
                        0, 0, 0x50, 0, // version 0.5
                        0, 3, // num_glyphs = 3
                    ]))
                } else if tag == Tag::new(b"hhea") {
                    Some(FontData::new(&[
                        0, 1, 0, 0, // version 1.0
                        0, 0, 0, 0, // ascender/descender
                        0, 0, 0, 0, // line gap/advance width
                        0, 0, 0, 0, // min left/right side bearing
                        0, 0, 0, 0, // x_max, caret_slope_rise
                        0, 0, 0, 0, // caret_slope_run, caret_offset
                        0, 0, 0, 0, // reserved1/2
                        0, 0, 0, 0, // reserved 3/4
                        0, 0, 0, 1, // metric format, number_of_h_metrics
                    ]))
                } else if tag == Tag::new(b"hmtx") {
                    Some(FontData::new(&[
                        0, 4, 0, 6, // LongHorMetric: 4, 6
                        0, 30, 0, 111, // two lsb entries
                    ]))
                } else {
                    None
                }
            }
        }

        let number_of_h_metrics = DummyProvider.hhea().unwrap().number_of_long_metrics();
        let num_glyphs = DummyProvider.maxp().unwrap().num_glyphs();
        let hmtx = DummyProvider.hmtx().unwrap();

        assert_eq!(number_of_h_metrics, 1);
        assert_eq!(num_glyphs, 3);
        assert_eq!(hmtx.long_metrics().len(), 1);
        assert_eq!(hmtx.bearings().len(), 2);
    }
}
