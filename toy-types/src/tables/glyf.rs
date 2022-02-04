use crate::*;
use zerocopy::{AsBytes, FromBytes, LayoutVerified, Unaligned, BE, I16};

#[derive(Clone, Debug, FontThing)]
pub struct GlyphHeader {
    pub number_of_contours: int16,
    pub x_min: int16,
    pub y_min: int16,
    pub x_max: int16,
    pub y_max: int16,
}

#[derive(FromBytes, AsBytes, Unaligned)]
#[repr(C)]
pub struct GlyphHeaderZero {
    pub number_of_contours: I16<BE>,
    pub x_min: I16<BE>,
    pub y_min: I16<BE>,
    pub x_max: I16<BE>,
    pub y_max: I16<BE>,
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

    pub fn get_zc(&self, offset: usize) -> Option<&'a GlyphHeaderZero> {
        let verified: LayoutVerified<_, GlyphHeaderZero> =
            self.data
                .get(offset..self.data.len())
                .and_then(|blob| LayoutVerified::new_unaligned(blob.as_bytes()))?;
        Some(verified.into_ref())
    }

    pub fn get_view(&self, offset: usize) -> Option<GlyphHeaderDerivedView> {
        self.data
            .get(offset..self.data.len())
            .and_then(<GlyphHeader as FontThing>::View::read)
    }
}
