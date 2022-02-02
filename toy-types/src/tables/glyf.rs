use crate::*;

#[derive(Clone, Debug, FontThing)]
pub struct GlyphHeader {
    pub number_of_contours: int16,
    pub x_min: int16,
    pub y_min: int16,
    pub x_max: int16,
    pub y_max: int16,
}

pub struct Glyf<'a> {
    data: Blob<'a>,
}

impl<'a> Glyf<'a> {
    pub fn new(data: Blob<'a>) -> Option<Self> {
        Some(Self { data })
    }

    pub fn get(&self, offset: usize) -> Option<GlyphHeader> {
        self.data
            .get(offset..self.data.len())
            .and_then(GlyphHeader::read)
    }

    pub fn get_view(&self, offset: usize) -> Option<GlyphHeaderDerivedView> {
        self.data
            .get(offset..self.data.len())
            .and_then(<GlyphHeader as FontThing>::View::read)
    }
}
