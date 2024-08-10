//! Common functionality for glyf, cff and autohinting scalers.

use raw::{
    tables::{hmtx::Hmtx, hvar::Hvar},
    types::{BigEndian, F2Dot14, GlyphId, Tag},
    FontRef, TableProvider,
};

/// Common functionality for glyf, cff and autohinting scalers.
#[derive(Clone)]
pub(crate) struct BaseScaler<'a> {
    pub font: FontRef<'a>,
    pub hmtx: Hmtx<'a>,
    pub hvar: Option<Hvar<'a>>,
}

impl<'a> BaseScaler<'a> {
    pub fn new(font: &FontRef<'a>) -> Option<Self> {
        let hmtx = font.hmtx().ok()?;
        let hvar = font.hvar().ok();
        Some(Self {
            font: font.clone(),
            hmtx,
            hvar,
        })
    }

    pub fn advance_width(&self, gid: GlyphId, coords: &'a [F2Dot14]) -> i32 {
        let mut advance = self.hmtx.advance(gid).unwrap_or_default() as i32;
        if let Some(hvar) = &self.hvar {
            advance += hvar
                .advance_width_delta(gid, coords)
                // FreeType truncates metric deltas...
                .map(|delta| delta.to_f64() as i32)
                .unwrap_or(0);
        }
        advance
    }

    pub fn lsb(&self, gid: GlyphId, coords: &'a [F2Dot14]) -> i32 {
        let mut lsb = self.hmtx.side_bearing(gid).unwrap_or_default() as i32;
        if let Some(hvar) = &self.hvar {
            lsb += hvar
                .lsb_delta(gid, coords)
                // FreeType truncates metric deltas...
                .map(|delta| delta.to_f64() as i32)
                .unwrap_or(0);
        }
        lsb
    }

    pub fn cvt(&self) -> &[BigEndian<i16>] {
        self.font
            .data_for_tag(Tag::new(b"cvt "))
            .and_then(|d| d.read_array(0..d.len()).ok())
            .unwrap_or_default()
    }
}
