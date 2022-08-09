//! a trait for things that can serve font tables

use font_types::Tag;

use crate::{tables, FontData, FontRead, FontReadWithArgs, ReadError};

/// An interface for accessing tables from a font (or font-like object)
pub trait TableProvider {
    fn data_for_tag(&self, tag: Tag) -> Option<FontData>;

    fn expect_data_for_tag(&self, tag: Tag) -> Result<FontData, ReadError> {
        self.data_for_tag(tag).ok_or(ReadError::TableIsMissing(tag))
    }

    fn head(&self) -> Result<tables::head::Head, ReadError> {
        self.expect_data_for_tag(tables::head::TAG)
            .and_then(FontRead::read)
    }

    fn name(&self) -> Result<tables::name::Name, ReadError> {
        self.expect_data_for_tag(tables::name::TAG)
            .and_then(FontRead::read)
    }

    //fn hhea(&self) -> Option<hhea::Hhea> {
    //self.data_for_tag(hhea::TAG).and_then(hhea::Hhea::read)
    //}

    //fn hmtx(&self) -> Option<hmtx::Hmtx> {
    ////FIXME: should we make the user pass these in?
    //let num_glyphs = self.maxp().map(|maxp| maxp.num_glyphs())?;
    //let number_of_h_metrics = self.hhea().map(|hhea| hhea.number_of_h_metrics())?;
    //self.data_for_tag(hmtx::TAG)
    //.and_then(|data| hmtx::Hmtx::read_with_args(data, &(num_glyphs, number_of_h_metrics)))
    //.map(|(table, _)| table)
    //}

    fn maxp(&self) -> Result<tables::maxp::Maxp, ReadError> {
        self.expect_data_for_tag(tables::maxp::TAG)
            .and_then(FontRead::read)
    }

    //fn post(&self) -> Option<post::Post> {
    //self.data_for_tag(post::TAG).and_then(post::Post::read)
    //}

    //fn stat(&self) -> Option<stat::Stat> {
    //self.data_for_tag(stat::TAG).and_then(stat::Stat::read)
    //}

    fn loca(&self, is_long: bool) -> Result<tables::loca::Loca, ReadError> {
        self.expect_data_for_tag(tables::loca::TAG)
            .and_then(|data| FontReadWithArgs::read_with_args(data, &is_long))
    }

    fn glyf(&self) -> Result<tables::glyf::Glyf, ReadError> {
        self.expect_data_for_tag(tables::glyf::TAG)
            .and_then(FontRead::read)
    }

    fn cmap(&self) -> Result<tables::cmap::Cmap, ReadError> {
        self.expect_data_for_tag(tables::cmap::TAG)
            .and_then(FontRead::read)
    }

    fn gdef(&self) -> Result<tables::gdef::Gdef, ReadError> {
        self.expect_data_for_tag(tables::gdef::TAG)
            .and_then(FontRead::read)
    }

    fn gpos(&self) -> Result<tables::gpos::Gpos, ReadError> {
        self.expect_data_for_tag(tables::gpos::TAG)
            .and_then(FontRead::read)
    }
}
