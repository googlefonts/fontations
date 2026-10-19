//! Autohinting state for a font instance.

use super::{
    super::{
        pen::PathStyle, AdjustedMetrics, DrawError, OutlineGlyphCollection, OutlinePen, Target,
    },
    context::{AutohintContext, FontRefContext},
    metrics::{fixed_mul, pix_round, Scale, UnscaledStyleMetricsSet},
    outline::Outline,
    shape::{Shaper, ShaperMode},
    style::{GlyphStyle, GlyphStyleMap},
    ScaleFlags,
};
use crate::{attribute::Style, prelude::Size};
use alloc::sync::Arc;
use raw::types::{F26Dot6, GlyphId};

/// We enable "best effort" mode by default but allow it to be disabled with
/// a feature for testing.
const SHAPER_MODE: ShaperMode = if cfg!(feature = "autohint_shaping") {
    ShaperMode::BestEffort
} else {
    ShaperMode::Nominal
};

/// Set of derived glyph styles that are used for automatic hinting.
///
/// These are invariant per font so can be precomputed and reused for multiple
/// instances when requesting automatic hinting with [`Engine::Auto`](super::super::hint::Engine::Auto).
#[derive(Clone, Debug)]
pub struct GlyphStyles(Arc<GlyphStyleMap>);

impl GlyphStyles {
    /// Precomputes the full set of glyph styles for the given outlines.
    pub fn new(outlines: &OutlineGlyphCollection) -> Self {
        if let Some(font) = outlines.font() {
            // The style table is sized by the glyph count, which a font can
            // overstate; it never needs more entries than there are outlines.
            Self::from_context(&FontRefContext::with_outlines(font, outlines, &[]))
        } else {
            Self(Default::default())
        }
    }

    /// Precomputes the full set of glyph styles for the font behind
    /// `context`.
    pub(crate) fn from_context(context: &dyn AutohintContext) -> Self {
        let shaper = Shaper::new(context, SHAPER_MODE);
        Self(Arc::new(GlyphStyleMap::new(context.glyph_count(), &shaper)))
    }

    /// Returns the style for the given glyph.
    pub fn get(&self, gid: GlyphId) -> Option<GlyphStyle> {
        self.0.style(gid)
    }
}

/// The data the automatic hinter derives from a font at one location.
///
/// It does not depend on the hinting target.
pub(crate) struct LocatedState {
    styles: GlyphStyles,
    /// Measured from the outlines at the location. Each style is measured
    /// when first drawn, or all at once without `std`.
    metrics: UnscaledStyleMetricsSet,
    is_fixed_width: bool,
    style: Style,
}

impl LocatedState {
    #[cfg_attr(not(feature = "std"), allow(unused_variables))]
    fn new(context: &dyn AutohintContext, styles: GlyphStyles, lazy_metrics: bool) -> Self {
        #[cfg(feature = "std")]
        let metrics = if lazy_metrics {
            UnscaledStyleMetricsSet::lazy(&styles.0)
        } else {
            UnscaledStyleMetricsSet::precomputed(context, SHAPER_MODE, &styles.0)
        };
        #[cfg(not(feature = "std"))]
        let metrics = UnscaledStyleMetricsSet::precomputed(context, SHAPER_MODE, &styles.0);
        Self {
            styles,
            metrics,
            is_fixed_width: context.is_fixed_pitch(),
            style: context.style(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Instance {
    state: Arc<LocatedState>,
    target: Target,
}

impl Instance {
    pub fn new(
        context: &dyn AutohintContext,
        target: Target,
        styles: Option<GlyphStyles>,
        lazy_metrics: bool,
    ) -> Self {
        let styles = styles.unwrap_or_else(|| GlyphStyles::from_context(context));
        Self {
            state: Arc::new(LocatedState::new(context, styles, lazy_metrics)),
            target,
        }
    }

    pub fn draw(
        &self,
        context: &dyn AutohintContext,
        size: Size,
        glyph_id: GlyphId,
        path_style: PathStyle,
        pen: &mut impl OutlinePen,
    ) -> Result<AdjustedMetrics, DrawError> {
        let state = &*self.state;
        let style = state
            .styles
            .0
            .style(glyph_id)
            .ok_or(DrawError::GlyphNotFound(glyph_id))?;
        let metrics = state
            .metrics
            .get(
                |compute| compute(context),
                SHAPER_MODE,
                &state.styles.0,
                glyph_id,
            )
            .ok_or(DrawError::GlyphNotFound(glyph_id))?;
        let mut outline = Outline::default();
        let has_overlaps = context
            .fill_outline(glyph_id, &mut outline, super::QuirksMode::default())
            .ok_or(DrawError::GlyphNotFound(glyph_id))??;
        let units_per_em = outline.units_per_em;
        let scale = Scale::new(
            size.ppem().unwrap_or(units_per_em as f32),
            units_per_em,
            state.style,
            self.target,
            metrics.style_class().script.group,
        );
        let hinted_metrics = super::hint::hint_outline(&mut outline, &metrics, &scale, Some(style));
        let h_advance = outline.advance;
        let mut pp1x = 0;
        let mut pp2x = fixed_mul(h_advance, hinted_metrics.x_scale);
        let is_light = self.target.is_light() || self.target.preserve_linear_metrics();
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afloader.c#L422>
        if !is_light {
            if let (true, Some(edge_metrics)) = (
                !scale.flags.contains(ScaleFlags::NO_ADVANCE),
                hinted_metrics.edge_metrics,
            ) {
                let old_rsb = pp2x - edge_metrics.right_opos;
                let old_lsb = edge_metrics.left_opos;
                let new_lsb = edge_metrics.left_pos;
                let mut pp1x_uh = new_lsb - old_lsb;
                let mut pp2x_uh = edge_metrics.right_pos + old_rsb;
                if old_lsb < 24 {
                    pp1x_uh -= 8;
                }
                if old_rsb < 24 {
                    pp2x_uh += 8;
                }
                pp1x = pix_round(pp1x_uh);
                pp2x = pix_round(pp2x_uh);
                if pp1x >= new_lsb && old_lsb > 0 {
                    pp1x -= 64;
                }
                if pp2x <= edge_metrics.right_pos && old_rsb > 0 {
                    pp2x += 64;
                }
            } else {
                pp1x = pix_round(pp1x);
                pp2x = pix_round(pp2x);
            }
        } else {
            pp1x = pix_round(pp1x);
            pp2x = pix_round(pp2x);
        }
        if pp1x != 0 {
            for point in &mut outline.points {
                point.x -= pp1x;
            }
        }
        let advance = if !is_light
            && (state.is_fixed_width || (metrics.digits_have_same_width && style.is_digit()))
        {
            fixed_mul(h_advance, scale.x_scale)
        } else if h_advance != 0 {
            pp2x - pp1x
        } else {
            0
        };
        outline.to_path(path_style, pen)?;
        Ok(AdjustedMetrics {
            has_overlaps,
            lsb: None,
            advance_width: Some(F26Dot6::from_bits(pix_round(advance)).to_f32()),
        })
    }
}
