//! Font tables.

pub mod cmap;
pub mod head;
pub mod hhea;
pub mod hmtx;
pub mod maxp;

use font_types::{FontRead, Tag};

/// An interface for accessing tables from a font (or font-like object)
pub trait TableProvider {
    fn data_for_tag(&self, tag: Tag) -> Option<&[u8]>;

    fn head(&self) -> Option<head::Head> {
        self.data_for_tag(Tag::new(b"head"))
            .and_then(head::Head::read)
    }

    fn hhea(&self) -> Option<hhea::Hhea> {
        self.data_for_tag(hhea::TAG).and_then(hhea::Hhea::read)
    }

    fn hmtx(&self) -> Option<hmtx::Hmtx> {
        //FIXME: should we make the user pass these in?
        let num_glyphs = self.maxp().map(|maxp| maxp.num_glyphs())?;
        let number_of_h_metrics = self.hhea().map(|hhea| hhea.number_of_h_metrics())?;
        self.data_for_tag(hmtx::TAG).and_then(|data| {
            hmtx::Hmtx::read(data, num_glyphs as usize, number_of_h_metrics as usize)
        })
    }

    fn maxp(&self) -> Option<maxp::Maxp> {
        self.data_for_tag(maxp::TAG).and_then(maxp::Maxp::read)
    }

    //fn loca(&self, is_32_bit: bool) -> Option<Loca>;
    //fn glyf(&self) -> Option<Glyf>;
    fn cmap(&self) -> Option<cmap::Cmap> {
        self.data_for_tag(Tag::new(b"cmap"))
            .and_then(cmap::Cmap::read)
    }
}
