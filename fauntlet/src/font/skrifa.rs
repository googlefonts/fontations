use super::{InstanceOptions, SharedFontData};
use crate::Font;
use skrifa::{
    outline::{DrawError, DrawSettings, HintingInstance, HintingOptions, OutlinePen},
    prelude::{LocationRef, Size},
    raw::{
        tables::postscript::{
            charstring::{CommandSink, NopFilterSink, TransformSink},
            font::Type1Font,
        },
        types::{F2Dot14, Fixed},
        FontRef, TableProvider,
    },
    GlyphId, MetadataProvider, OutlineGlyphCollection,
};

#[allow(clippy::large_enum_variant)]
pub enum SkrifaInstance<'a> {
    Sfnt(SkrifaSfntInstance<'a>),
    Type1(SkrifaType1Instance<'a>),
}

impl<'a> SkrifaInstance<'a> {
    pub fn new(font: &'a Font, options: &InstanceOptions) -> Option<Self> {
        if let Some(type1) = font.type1.as_ref() {
            Some(Self::Type1(SkrifaType1Instance {
                font: type1,
                ppem: (options.ppem != 0.0).then_some(options.ppem),
            }))
        } else {
            SkrifaSfntInstance::new(&font.data, options).map(Self::Sfnt)
        }
    }

    pub fn is_tricky(&self) -> bool {
        match self {
            Self::Sfnt(sfnt) => sfnt.is_tricky(),
            _ => false,
        }
    }

    pub fn glyph_count(&self) -> u16 {
        match self {
            Self::Sfnt(sfnt) => sfnt.glyph_count(),
            Self::Type1(type1) => type1.font.num_glyphs() as _,
        }
    }

    pub fn advance(&mut self, glyph_id: GlyphId) -> Option<f32> {
        match self {
            Self::Sfnt(sfnt) => sfnt.advance(glyph_id),
            _ => None,
        }
    }

    pub fn hvar_and_gvar_advance_deltas(&self, glyph_id: GlyphId) -> Option<(i32, i32)> {
        match self {
            Self::Sfnt(sfnt) => sfnt.hvar_and_gvar_advance_deltas(glyph_id),
            _ => None,
        }
    }

    /// Returns the scaler adjusted advance width when available.
    pub fn outline(
        &mut self,
        glyph_id: GlyphId,
        pen: &mut impl OutlinePen,
    ) -> Result<Option<f32>, DrawError> {
        match self {
            Self::Sfnt(sfnt) => sfnt.outline(glyph_id, pen),
            Self::Type1(type1) => type1.outline(glyph_id, pen),
        }
    }
}

pub struct SkrifaSfntInstance<'a> {
    font: FontRef<'a>,
    size: Size,
    coords: Vec<F2Dot14>,
    outlines: OutlineGlyphCollection<'a>,
    hinter: Option<HintingInstance>,
}

impl<'a> SkrifaSfntInstance<'a> {
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
                    )
                    .ok()?,
                )
            } else if let Some(hinting_options) = options.hinting.skrifa_options() {
                Some(HintingInstance::new(&outlines, size, options.coords, hinting_options).ok()?)
            } else {
                None
            }
        } else {
            None
        };
        Some(Self {
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

pub struct SkrifaType1Instance<'a> {
    font: &'a Type1Font,
    ppem: Option<f32>,
}

impl SkrifaType1Instance<'_> {
    pub fn outline(
        &mut self,
        glyph_id: GlyphId,
        pen: &mut impl OutlinePen,
    ) -> Result<Option<f32>, DrawError> {
        let mut pen = PenCommandSink(pen);
        let mut nop_filter = NopFilterSink::new(&mut pen);
        let scale = self.ppem.map(|ppem| self.font.scale_for_ppem(ppem));
        let mut transformer = TransformSink::new(&mut nop_filter, self.font.matrix(), scale);
        let width = self
            .font
            .evaluate_charstring(glyph_id, &mut transformer)
            .map_err(DrawError::PostScript)?;
        let width = width.map(|w| self.font.transform_h_metric(scale, w));
        Ok(width.map(|w| w.to_f32().max(0.0)))
    }
}

struct PenCommandSink<'a, P: OutlinePen>(&'a mut P);

impl<P: OutlinePen> CommandSink for PenCommandSink<'_, P> {
    fn move_to(&mut self, x: Fixed, y: Fixed) {
        self.0.move_to(x.to_f32(), y.to_f32());
    }

    fn line_to(&mut self, x: Fixed, y: Fixed) {
        self.0.line_to(x.to_f32(), y.to_f32());
    }

    fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed) {
        self.0.curve_to(
            cx0.to_f32(),
            cy0.to_f32(),
            cx1.to_f32(),
            cy1.to_f32(),
            x.to_f32(),
            y.to_f32(),
        );
    }

    fn close(&mut self) {
        self.0.close()
    }
}
