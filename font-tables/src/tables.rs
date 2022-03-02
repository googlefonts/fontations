//! Font tables.

pub mod cmap;
pub mod head;
pub mod hhea;
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
