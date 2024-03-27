use ::skrifa::{
    outline::{DrawError, DrawSettings, EmbeddedHintingInstance},
    prelude::{LocationRef, Size},
    raw::types::F2Dot14,
    raw::{types::Pen, FontRef, TableProvider},
    GlyphId, MetadataProvider, OutlineGlyphCollection,
};

use super::{InstanceOptions, SharedFontData};

pub struct SkrifaInstance<'a> {
    font: FontRef<'a>,
    size: Size,
    coords: Vec<F2Dot14>,
    outlines: OutlineGlyphCollection<'a>,
    hinter: Option<EmbeddedHintingInstance>,
}

impl<'a> SkrifaInstance<'a> {
    pub fn new(data: &'a SharedFontData, options: &InstanceOptions) -> Option<Self> {
        let font = FontRef::from_index(data.0.as_ref(), options.index as u32).ok()?;
        let size = if options.ppem != 0 {
            Size::new(options.ppem as f32)
        } else {
            Size::unscaled()
        };
        let outlines = font.outline_glyphs();
        let hinter = if options.ppem != 0 && options.hinting.is_some() {
            Some(
                EmbeddedHintingInstance::new(
                    &outlines,
                    size,
                    options.coords,
                    options.hinting.unwrap(),
                )
                .ok()?,
            )
        } else {
            None
        };
        Some(SkrifaInstance {
            font,
            size,
            coords: options.coords.into(),
            outlines,
            hinter,
        })
    }

    pub fn glyph_count(&self) -> u16 {
        self.font
            .maxp()
            .map(|maxp| maxp.num_glyphs())
            .unwrap_or_default()
    }

    pub fn advance(&mut self, glyph_id: GlyphId) -> Option<f32> {
        self.font
            .glyph_metrics(self.size, LocationRef::new(&self.coords))
            .advance_width(glyph_id)
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Result<(), DrawError> {
        let outline = self
            .outlines
            .get(glyph_id)
            .ok_or(DrawError::GlyphNotFound(glyph_id))?;
        if let Some(hinter) = self.hinter.as_ref() {
            outline.draw(DrawSettings::hinted(hinter, false), pen)?;
        } else {
            outline.draw((self.size, self.coords.as_slice()), pen)?;
        }
        Ok(())
    }
}
