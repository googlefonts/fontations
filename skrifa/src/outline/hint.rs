//! Support for applying embedded hinting instructions.

use super::{
    autohint::{self, GlyphStyles},
    cff,
    glyf::{self, FreeTypeScaler},
    pen::PathStyle,
    AdjustedMetrics, DrawError, Hinting, LocationRef, NormalizedCoord, OutlineCollectionKind,
    OutlineGlyph, OutlineGlyphCollection, OutlineKind, OutlinePen, Size,
};
use crate::alloc::{boxed::Box, vec::Vec};

/// Configuration settings for a hinting instance.
#[derive(Clone, Default, Debug)]
pub struct HintingOptions {
    /// Specifies the hinting engine to use.
    ///
    /// Defaults to [`Engine::AutoFallback`].
    pub engine: Engine,
    /// Defines the properties of the intended target of a hinted outline.
    ///
    /// Defaults to a target with [`SmoothMode::Normal`] which is equivalent
    /// to `FT_RENDER_MODE_NORMAL` in FreeType.
    pub target: Target,
}

impl From<Target> for HintingOptions {
    fn from(value: Target) -> Self {
        Self {
            engine: Engine::AutoFallback,
            target: value,
        }
    }
}

/// Specifies the backend to use when applying hints.
#[derive(Clone, Default, Debug)]
pub enum Engine {
    /// The TrueType or PostScript interpreter.
    Interpreter,
    /// The automatic hinter that performs just-in-time adjustment of
    /// outlines.
    ///
    /// Glyph styles can be precomputed per font and may be provided here
    /// as an optimization to avoid recomputing them for each instance.
    Auto(Option<GlyphStyles>),
    /// Selects the engine based on the same rules that FreeType uses in
    /// the default configuration.
    ///
    /// Specifically, PostScript fonts will always use the interpreter and
    /// TrueType fonts will use the interpreter if one of the `fpgm` or `prep`
    /// tables is non-empty, falling back to the automatic hinter otherwise.
    ///
    /// This uses [`OutlineGlyphCollection::prefer_interpreter`] to make a
    /// selection.
    #[default]
    AutoFallback,
}

impl Engine {
    fn resolve_auto_fallback(self, outlines: &OutlineGlyphCollection) -> Engine {
        match self {
            Self::Interpreter => Self::Interpreter,
            Self::Auto(styles) => Self::Auto(styles),
            Self::AutoFallback => {
                if outlines.prefer_interpreter() {
                    Self::Interpreter
                } else {
                    Self::Auto(None)
                }
            }
        }
    }
}

impl From<Engine> for HintingOptions {
    fn from(value: Engine) -> Self {
        Self {
            engine: value,
            target: Default::default(),
        }
    }
}

/// Defines the target settings for hinting.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Target {
    /// Strong hinting style that should only be used for aliased, monochromatic
    /// rasterization.
    ///
    /// Corresponds to `FT_LOAD_TARGET_MONO` in FreeType.
    Mono,
    /// Hinting style that is suitable for anti-aliased rasterization.
    ///
    /// Corresponds to the non-monochrome load targets in FreeType. See
    /// [`SmoothMode`] for more detail.
    Smooth {
        /// The basic mode for smooth hinting.
        ///
        /// Defaults to [`SmoothMode::Normal`].
        mode: SmoothMode,
        /// If true, TrueType bytecode may assume that the resulting outline
        /// will be rasterized with supersampling in the vertical direction.
        ///
        /// When this is enabled, ClearType fonts will often generate wider
        /// horizontal stems that may lead to blurry images when rendered with
        /// an analytical area rasterizer (such as the one in FreeType).
        ///
        /// FreeType has no corresponding setting and behaves as if this is
        /// always enabled.
        ///
        /// This only applies to the TrueType interpreter.
        ///
        /// Defaults to `true`.
        symmetric_rendering: bool,
        /// If true, prevents adjustment of the outline in the horizontal
        /// direction and preserves inter-glyph spacing.
        ///
        /// This is useful for performing layout without concern that hinting
        /// will modify the advance width of a glyph. Specifically, it means
        /// that layout will not require evaluation of glyph outlines.
        ///
        /// FreeType has no corresponding setting and behaves as if this is
        /// always disabled.
        ///
        /// This applies to the TrueType interpreter and the automatic hinter.
        ///
        /// Defaults to `false`.       
        preserve_linear_metrics: bool,
    },
}

impl Default for Target {
    fn default() -> Self {
        SmoothMode::Normal.into()
    }
}

// Helpers for deriving various flags from the mode which
// change the behavior of certain instructions.
// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttgload.c#L2222>
impl Target {
    pub(crate) fn is_smooth(&self) -> bool {
        matches!(self, Self::Smooth { .. })
    }

    pub(crate) fn is_grayscale_cleartype(&self) -> bool {
        match self {
            Self::Smooth { mode, .. } => matches!(mode, SmoothMode::Normal | SmoothMode::Light),
            _ => false,
        }
    }

    pub(crate) fn is_light(&self) -> bool {
        matches!(
            self,
            Self::Smooth {
                mode: SmoothMode::Light,
                ..
            }
        )
    }

    pub(crate) fn is_lcd(&self) -> bool {
        matches!(
            self,
            Self::Smooth {
                mode: SmoothMode::Lcd,
                ..
            }
        )
    }

    pub(crate) fn is_vertical_lcd(&self) -> bool {
        matches!(
            self,
            Self::Smooth {
                mode: SmoothMode::VerticalLcd,
                ..
            }
        )
    }

    pub(crate) fn symmetric_rendering(&self) -> bool {
        matches!(
            self,
            Self::Smooth {
                symmetric_rendering: true,
                ..
            }
        )
    }

    pub(crate) fn preserve_linear_metrics(&self) -> bool {
        matches!(
            self,
            Self::Smooth {
                preserve_linear_metrics: true,
                ..
            }
        )
    }
}

/// Mode selector for a smooth hinting target.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum SmoothMode {
    /// The standard smooth hinting mode.
    ///
    /// Corresponds to `FT_LOAD_TARGET_NORMAL` in FreeType.
    #[default]
    Normal,
    /// Hinting with a lighter touch, typically meaning less aggressive
    /// adjustment in the horizontal direction.
    ///
    /// Corresponds to `FT_LOAD_TARGET_LIGHT` in FreeType.
    Light,
    /// Hinting that is optimized for subpixel rendering with horizontal LCD
    /// layouts.
    ///
    /// Corresponds to `FT_LOAD_TARGET_LCD` in FreeType.
    Lcd,
    /// Hinting that is optimized for subpixel rendering with vertical LCD
    /// layouts.
    ///
    /// Corresponds to `FT_LOAD_TARGET_LCD_V` in FreeType.
    VerticalLcd,
}

impl From<SmoothMode> for Target {
    fn from(value: SmoothMode) -> Self {
        Self::Smooth {
            mode: value,
            symmetric_rendering: true,
            preserve_linear_metrics: false,
        }
    }
}

impl From<HintingMode> for HintingOptions {
    fn from(value: HintingMode) -> Self {
        match value {
            HintingMode::Strong => Target::Mono.into(),
            HintingMode::Smooth {
                lcd_subpixel,
                preserve_linear_metrics,
            } => {
                let mode = match lcd_subpixel {
                    Some(LcdLayout::Horizontal) => SmoothMode::Lcd,
                    Some(LcdLayout::Vertical) => SmoothMode::VerticalLcd,
                    _ => SmoothMode::Normal,
                };
                Target::Smooth {
                    mode,
                    symmetric_rendering: true,
                    preserve_linear_metrics,
                }
                .into()
            }
        }
    }
}

/// Modes that control hinting when using embedded instructions.
///
/// Only the TrueType interpreter supports all hinting modes.
///
/// # FreeType compatibility
///
/// The following table describes how to map FreeType hinting modes:
///
/// | FreeType mode         | Variant                                                                              |
/// |-----------------------|--------------------------------------------------------------------------------------|
/// | FT_LOAD_TARGET_MONO   | Strong                                                                               |
/// | FT_LOAD_TARGET_NORMAL | Smooth { lcd_subpixel: None, preserve_linear_metrics: false }                        |
/// | FT_LOAD_TARGET_LCD    | Smooth { lcd_subpixel: Some(LcdLayout::Horizontal), preserve_linear_metrics: false } |
/// | FT_LOAD_TARGET_LCD_V  | Smooth { lcd_subpixel: Some(LcdLayout::Vertical), preserve_linear_metrics: false }   |
///
/// Note: `FT_LOAD_TARGET_LIGHT` is equivalent to `FT_LOAD_TARGET_NORMAL` since
/// FreeType 2.7.
///
/// The default value of this type is equivalent to `FT_LOAD_TARGET_NORMAL`.
#[doc(hidden)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HintingMode {
    /// Strong hinting mode that should only be used for aliased, monochromatic
    /// rasterization.
    ///
    /// Corresponds to `FT_LOAD_TARGET_MONO` in FreeType.
    Strong,
    /// Lighter hinting mode that is intended for anti-aliased rasterization.
    Smooth {
        /// If set, enables support for optimized hinting that takes advantage
        /// of subpixel layouts in LCD displays and corresponds to
        /// `FT_LOAD_TARGET_LCD` or `FT_LOAD_TARGET_LCD_V` in FreeType.
        ///
        /// If unset, corresponds to `FT_LOAD_TARGET_NORMAL` in FreeType.
        lcd_subpixel: Option<LcdLayout>,
        /// If true, prevents adjustment of the outline in the horizontal
        /// direction and preserves inter-glyph spacing.
        ///
        /// This is useful for performing layout without concern that hinting
        /// will modify the advance width of a glyph. Specifically, it means
        /// that layout will not require evaluation of glyph outlines.
        ///
        /// FreeType has no corresponding setting.
        preserve_linear_metrics: bool,
    },
}

impl Default for HintingMode {
    fn default() -> Self {
        Self::Smooth {
            lcd_subpixel: None,
            preserve_linear_metrics: false,
        }
    }
}

/// Specifies direction of pixel layout for LCD based subpixel hinting.
#[doc(hidden)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LcdLayout {
    /// Subpixels are ordered horizontally.
    ///
    /// Corresponds to `FT_LOAD_TARGET_LCD` in FreeType.
    Horizontal,
    /// Subpixels are ordered vertically.
    ///
    /// Corresponds to `FT_LOAD_TARGET_LCD_V` in FreeType.
    Vertical,
}

/// Hinting instance that uses information embedded in the font to perform
/// grid-fitting.
#[derive(Clone)]
pub struct HintingInstance {
    size: Size,
    coords: Vec<NormalizedCoord>,
    target: Target,
    kind: HinterKind,
}

impl HintingInstance {
    /// Creates a new embedded hinting instance for the given outline
    /// collection, size, location in variation space and hinting mode.
    pub fn new<'a>(
        outline_glyphs: &OutlineGlyphCollection,
        size: Size,
        location: impl Into<LocationRef<'a>>,
        options: impl Into<HintingOptions>,
    ) -> Result<Self, DrawError> {
        let options = options.into();
        let mut hinter = Self {
            size: Size::unscaled(),
            coords: vec![],
            target: options.target,
            kind: HinterKind::None,
        };
        hinter.reconfigure(outline_glyphs, size, location, options)?;
        Ok(hinter)
    }

    /// Returns the currently configured size.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Returns the currently configured normalized location in variation space.
    pub fn location(&self) -> LocationRef {
        LocationRef::new(&self.coords)
    }

    /// Resets the hinter state for a new font instance with the given
    /// outline collection and settings.
    pub fn reconfigure<'a>(
        &mut self,
        outlines: &OutlineGlyphCollection,
        size: Size,
        location: impl Into<LocationRef<'a>>,
        options: impl Into<HintingOptions>,
    ) -> Result<(), DrawError> {
        self.size = size;
        self.coords.clear();
        self.coords.extend_from_slice(location.into().coords());
        let options = options.into();
        self.target = options.target;
        let engine = options.engine.resolve_auto_fallback(outlines);
        // Reuse memory if the font contains the same outline format
        let current_kind = core::mem::replace(&mut self.kind, HinterKind::None);
        match engine {
            Engine::Interpreter => match &outlines.kind {
                OutlineCollectionKind::Glyf(glyf) => {
                    let mut hint_instance = match current_kind {
                        HinterKind::Glyf(instance) => instance,
                        _ => Box::<glyf::HintInstance>::default(),
                    };
                    let ppem = size.ppem();
                    let scale = glyf.compute_scale(ppem).1.to_bits();
                    hint_instance.reconfigure(
                        glyf,
                        scale,
                        ppem.unwrap_or_default() as i32,
                        self.target,
                        &self.coords,
                    )?;
                    self.kind = HinterKind::Glyf(hint_instance);
                }
                OutlineCollectionKind::Cff(cff) => {
                    let mut subfonts = match current_kind {
                        HinterKind::Cff(subfonts) => subfonts,
                        _ => vec![],
                    };
                    subfonts.clear();
                    let ppem = size.ppem();
                    for i in 0..cff.subfont_count() {
                        subfonts.push(cff.subfont(i, ppem, &self.coords)?);
                    }
                    self.kind = HinterKind::Cff(subfonts);
                }
                OutlineCollectionKind::None => {}
            },
            Engine::Auto(styles) => {
                let Some(font) = outlines.base_outlines().map(|scaler| &scaler.font) else {
                    return Ok(());
                };
                let instance = autohint::Instance::new(
                    font,
                    outlines,
                    &self.coords,
                    self.target,
                    styles,
                    true,
                );
                self.kind = HinterKind::Auto(instance);
            }
            _ => {}
        }
        Ok(())
    }

    /// Returns true if hinting should actually be applied for this instance.
    ///
    /// Some TrueType fonts disable hinting dynamically based on the instance
    /// configuration.
    pub fn is_enabled(&self) -> bool {
        match &self.kind {
            HinterKind::Glyf(instance) => instance.is_enabled(),
            HinterKind::Cff(_) | HinterKind::Auto(_) => true,
            _ => false,
        }
    }

    pub(super) fn draw(
        &self,
        glyph: &OutlineGlyph,
        memory: Option<&mut [u8]>,
        path_style: PathStyle,
        pen: &mut impl OutlinePen,
        is_pedantic: bool,
    ) -> Result<AdjustedMetrics, DrawError> {
        let ppem = self.size.ppem();
        let coords = self.coords.as_slice();
        match (&self.kind, &glyph.kind) {
            (HinterKind::Auto(instance), _) => {
                instance.draw(self.size, coords, glyph, path_style, pen)
            }
            (HinterKind::Glyf(instance), OutlineKind::Glyf(glyf, outline)) => {
                if matches!(path_style, PathStyle::HarfBuzz) {
                    return Err(DrawError::HarfBuzzHintingUnsupported);
                }
                super::with_glyf_memory(outline, Hinting::Embedded, memory, |buf| {
                    let scaled_outline = FreeTypeScaler::hinted(
                        glyf.clone(),
                        outline,
                        buf,
                        ppem,
                        coords,
                        instance,
                        is_pedantic,
                    )?
                    .scale(&outline.glyph, outline.glyph_id)?;
                    scaled_outline.to_path(path_style, pen)?;
                    Ok(AdjustedMetrics {
                        has_overlaps: outline.has_overlaps,
                        lsb: Some(scaled_outline.adjusted_lsb().to_f32()),
                        advance_width: Some(scaled_outline.adjusted_advance_width().to_f32()),
                    })
                })
            }
            (HinterKind::Cff(subfonts), OutlineKind::Cff(cff, glyph_id, subfont_ix)) => {
                let Some(subfont) = subfonts.get(*subfont_ix as usize) else {
                    return Err(DrawError::NoSources);
                };
                cff.draw(subfont, *glyph_id, &self.coords, true, pen)?;
                Ok(AdjustedMetrics::default())
            }
            _ => Err(DrawError::NoSources),
        }
    }
}

#[derive(Clone)]
enum HinterKind {
    /// Represents a hinting instance that is associated with an empty outline
    /// collection.
    None,
    Glyf(Box<glyf::HintInstance>),
    Cff(Vec<cff::Subfont>),
    Auto(autohint::Instance),
}

#[cfg(test)]
mod tests {
    use crate::MetadataProvider;

    use super::*;
    use raw::{types::GlyphId, FontRef};

    #[test]
    fn autohinter_hebrew() {
        let font = FontRef::new(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS).unwrap();
        let outlines = font.outline_glyphs();
        let instance = HintingInstance::new(
            &outlines,
            Size::new(16.0),
            LocationRef::default(),
            Engine::Auto(None),
        )
        .unwrap();
        let glyph = outlines.get(GlyphId::new(9)).unwrap();
        let mut pen = super::super::pen::SvgPen::with_precision(6);
        let metrics = glyph.draw(&instance, &mut pen).unwrap();
        assert_eq!(metrics.advance_width, Some(320.0 / 64.0));
        let svg = pen.to_string();
        assert_eq!(
            svg,
            "M3.078125,-4.000000 \
            L3.078125,4.406250 \
            Q3.078125,5.359375 3.171875,6.046875 \
            Q3.281250,6.734375 3.468750,7.234375 \
            L3.468750,7.234375 \
            L1.890625,7.234375 \
            Q1.468750,7.234375 1.234375,7.484375 \
            Q1.000000,7.734375 1.000000,8.343750 \
            Q1.000000,8.562500 1.015625,8.734375 \
            Q1.031250,8.906250 1.093750,9.171875 \
            Q1.171875,9.437500 1.265625,9.890625 \
            L1.781250,9.890625 \
            L1.781250,9.828125 \
            Q1.781250,9.437500 1.984375,9.218750 \
            Q2.203125,9.000000 2.578125,9.000000 \
            L3.546875,9.000000 \
            Q3.812500,9.000000 3.906250,8.890625 \
            Q4.000000,8.781250 4.000000,8.468750 \
            L4.000000,7.421875 \
            Q3.968750,7.140625 3.937500,6.875000 \
            Q3.921875,6.609375 3.921875,6.156250 \
            Q3.921875,5.718750 3.921875,4.921875 \
            L3.921875,-3.437500 \
            Q3.781250,-3.609375 3.625000,-3.734375 \
            Q3.484375,-3.875000 3.281250,-4.000000 \
            Z"
        );
    }
}
