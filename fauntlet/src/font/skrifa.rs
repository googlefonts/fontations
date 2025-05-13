use ::skrifa::{
    outline::{DrawError, DrawSettings, HintingInstance, OutlinePen},
    prelude::{LocationRef, Size},
    raw::types::F2Dot14,
    raw::{FontRef, TableProvider},
    GlyphId, MetadataProvider, OutlineGlyphCollection,
};
use skrifa::outline::{HintingOptions, InterpreterVersion};

use super::{InstanceOptions, SharedFontData};

pub struct SkrifaInstance<'a> {
    font: FontRef<'a>,
    size: Size,
    coords: Vec<F2Dot14>,
    outlines: OutlineGlyphCollection<'a>,
    hinter: Option<HintingInstance>,
}

impl<'a> SkrifaInstance<'a> {
    pub fn new(data: &'a SharedFontData, options: &InstanceOptions) -> Option<Self> {
        let font = FontRef::from_index(data.0.as_ref(), options.index as u32).ok()?;
        let size = if options.ppem != 0.0 {
            Size::new(options.ppem)
        } else {
            Size::unscaled()
        };
        let outlines = font.outline_glyphs();
        let hinter = if options.ppem != 0.0 {
            if outlines.require_interpreter() {
                // In this case, we must use the interpreter to match FreeType
                Some(
                    HintingInstance::new(
                        &outlines,
                        size,
                        options.coords,
                        HintingOptions {
                            engine: skrifa::outline::Engine::Interpreter,
                            target: skrifa::outline::Target::Mono,
                        },
                        InterpreterVersion::_40,
                    )
                    .ok()?,
                )
            } else if let Some(hinting_options) = options.hinting.skrifa_options() {
                Some(
                    HintingInstance::new(
                        &outlines,
                        size,
                        options.coords,
                        hinting_options,
                        InterpreterVersion::_40,
                    )
                    .ok()?,
                )
            } else {
                None
            }
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

    pub fn is_tricky(&self) -> bool {
        self.font.outline_glyphs().require_interpreter()
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

    pub fn hvar_and_gvar_advance_deltas(&self, glyph_id: GlyphId) -> Option<(i32, i32)> {
        let hvar = self.font.hvar().ok()?;
        let gvar = self.font.gvar().ok()?;
        let hvar_delta = hvar.advance_width_delta(glyph_id, &self.coords).ok()?;
        let gvar_delta = gvar
            .phantom_point_deltas(
                &self.font.glyf().ok()?,
                &self.font.loca(None).ok()?,
                &self.coords,
                glyph_id,
            )
            .ok()??[1]
            .x;
        Some((hvar_delta.to_i32(), gvar_delta.to_i32()))
    }

    /// Returns the scaler adjusted advance width when available.
    pub fn outline(
        &mut self,
        glyph_id: GlyphId,
        pen: &mut impl OutlinePen,
    ) -> Result<Option<f32>, DrawError> {
        let outline = self
            .outlines
            .get(glyph_id)
            .ok_or(DrawError::GlyphNotFound(glyph_id))?;
        let draw_settings = if let Some(hinter) = self.hinter.as_ref() {
            DrawSettings::hinted(hinter, false)
        } else {
            DrawSettings::unhinted(self.size, self.coords.as_slice())
        };
        outline
            .draw(draw_settings, pen)
            .map(|metrics| metrics.advance_width)
    }
}
