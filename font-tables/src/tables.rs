//! Font tables.

use font_types::{FontRead, Tag};

/// An interface for accessing tables from a font (or font-like object)
pub trait TableProvider {
    fn data_for_tag(&self, tag: Tag) -> Option<&[u8]>;
    fn head(&self) -> Option<head::Head> {
        self.data_for_tag(Tag::new(b"head"))
            .and_then(head::Head::read)
    }
    //fn maxp(&self) -> Option<maxp::Maxp05>;
    //fn loca(&self, is_32_bit: bool) -> Option<Loca>;
    //fn glyf(&self) -> Option<Glyf>;
    //fn cmap(&self) -> Option<Cmap>;
}

pub mod head;
