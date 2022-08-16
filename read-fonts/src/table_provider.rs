//! a trait for things that can serve font tables

use font_types::Tag;

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

    fn hhea(&self) -> Result<tables::hhea::Hhea<'a>, ReadError> {
        self.expect_data_for_tag(tables::hhea::TAG)
            .and_then(FontRead::read)
    }

    fn hmtx(&self) -> Result<tables::hmtx::Hmtx<'a>, ReadError> {
        //FIXME: should we make the user pass these in?
        let num_glyphs = self.maxp().map(|maxp| maxp.num_glyphs())?;
        let number_of_h_metrics = self.hhea().map(|hhea| hhea.number_of_h_metrics())?;
        self.expect_data_for_tag(tables::hmtx::TAG)
            .and_then(|data| {
                FontReadWithArgs::read_with_args(data, &(num_glyphs, number_of_h_metrics))
            })
    }

    fn maxp(&self) -> Result<tables::maxp::Maxp<'a>, ReadError> {
        self.expect_data_for_tag(tables::maxp::TAG)
            .and_then(FontRead::read)
    }

    fn post(&self) -> Result<tables::post::Post<'a>, ReadError> {
        self.expect_data_for_tag(tables::post::TAG)
            .and_then(FontRead::read)
    }

    //fn stat(&self) -> Option<stat::Stat> {
    //self.data_for_tag(stat::TAG).and_then(stat::Stat::read)
    //}

    fn loca(&self, is_long: bool) -> Result<tables::loca::Loca<'a>, ReadError> {
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
}
