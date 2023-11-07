use ::skrifa::{
    prelude::{LocationRef, Size},
    raw::{types::Pen, FontRef, TableProvider},
    scale, GlyphId, MetadataProvider,
};

use super::{InstanceOptions, SharedFontData};

pub struct SkrifaInstance<'a> {
    font: FontRef<'a>,
    ppem: f32,
    scaler: scale::Scaler<'a>,
}

impl<'a> SkrifaInstance<'a> {
    pub fn new(
        data: &'a SharedFontData,
        options: &InstanceOptions,
        scaler_cx: &'a mut scale::Context,
    ) -> Option<Self> {
        let font = FontRef::from_index(data.0.as_ref(), options.index as u32).ok()?;
        let size = if options.ppem != 0 {
            Size::new(options.ppem as f32)
        } else {
            Size::unscaled()
        };
        let scaler = scaler_cx
            .new_scaler()
            .size(size)
            .normalized_coords(options.coords)
            .build(&font);
        Some(SkrifaInstance {
            font,
            ppem: size.ppem().unwrap_or_default(),
            scaler,
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
            .glyph_metrics(
                Size::new(self.ppem),
                LocationRef::new(self.scaler.normalized_coords()),
            )
            .advance_width(glyph_id)
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Option<()> {
        self.scaler.outline(glyph_id, pen).ok()?;
        Some(())
    }
}
