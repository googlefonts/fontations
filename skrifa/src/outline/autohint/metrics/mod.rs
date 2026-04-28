//! Autohinting specific metrics.

mod blues;
mod scale;
mod widths;

use super::{
    super::Target,
    shape::{Shaper, ShaperMode},
    style::{GlyphStyleMap, ScriptGroup, StyleClass},
    topo::Dimension,
};
use crate::{attribute::Style, collections::SmallVec, FontRef};
use alloc::vec::Vec;
use raw::types::{F2Dot14, Fixed, GlyphId};
#[cfg(feature = "std")]
use std::sync::{Arc, RwLock};

pub(crate) use blues::{BlueZones, ScaledBlue, ScaledBlues, UnscaledBlue, UnscaledBlues};
pub(crate) use scale::{compute_unscaled_style_metrics, scale_style_metrics};

#[cfg(feature = "autohinter")]
/// Public snapshot of a single unscaled blue zone.
#[derive(Clone, Debug, Default)]
pub struct ExportedUnscaledBlue {
    pub reference: i32,
    pub shoot: i32,
    pub is_adjustment: bool,
}

#[cfg(feature = "autohinter")]
/// Public snapshot of unscaled style metrics needed by ttfautohint.
#[derive(Clone, Debug, Default)]
pub struct ExportedUnscaledStyleMetrics {
    pub digits_have_same_width: bool,
    pub horizontal_widths: Vec<i32>,
    pub vertical_widths: Vec<i32>,
    pub blues: Vec<ExportedUnscaledBlue>,
}

#[cfg(feature = "autohinter")]
/// Public snapshot of a single scaled width entry.
#[derive(Clone, Debug, Default)]
pub struct ExportedScaledWidth {
    pub scaled: i32,
    pub fitted: i32,
}

#[cfg(feature = "autohinter")]
/// Public snapshot of a single scaled blue zone.
#[derive(Clone, Debug, Default)]
pub struct ExportedScaledBlue {
    pub reference_scaled: i32,
    pub reference_fitted: i32,
    pub shoot_scaled: i32,
    pub shoot_fitted: i32,
    pub is_active: bool,
    pub is_top: bool,
    pub is_sub_top: bool,
    pub is_neutral: bool,
    pub is_adjustment: bool,
}

#[cfg(feature = "autohinter")]
/// Public snapshot of scaled style metrics needed by ttfautohint.
#[derive(Clone, Debug, Default)]
pub struct ExportedScaledStyleMetrics {
    pub digits_have_same_width: bool,
    pub x_scale: i32,
    pub y_scale: i32,
    pub x_delta: i32,
    pub y_delta: i32,
    pub horizontal_widths: Vec<ExportedScaledWidth>,
    pub vertical_widths: Vec<ExportedScaledWidth>,
    pub blues: Vec<ExportedScaledBlue>,
}

#[cfg(feature = "autohinter")]
/// Compute unscaled style metrics for a style class and expose a stable,
/// allocation-owned representation suitable for FFI consumers.
pub fn compute_unscaled_style_metrics_exported(
    font: &FontRef,
    coords: &[F2Dot14],
    style: &StyleClass,
) -> ExportedUnscaledStyleMetrics {
    let shaper_mode = if cfg!(feature = "autohint_shaping") {
        ShaperMode::BestEffort
    } else {
        ShaperMode::Nominal
    };
    let shaper = Shaper::new(font, shaper_mode);
    let metrics = compute_unscaled_style_metrics(&shaper, coords, style);

    ExportedUnscaledStyleMetrics {
        digits_have_same_width: metrics.digits_have_same_width,
        horizontal_widths: metrics.axes[0].widths.iter().copied().collect(),
        vertical_widths: metrics.axes[1].widths.iter().copied().collect(),
        blues: metrics.axes[1]
            .blues
            .iter()
            .map(|blue| ExportedUnscaledBlue {
                reference: blue.position,
                shoot: blue.overshoot,
                is_adjustment: blue.zones.contains(BlueZones::ADJUSTMENT),
            })
            .collect(),
    }
}

#[cfg(feature = "autohinter")]
#[allow(clippy::too_many_arguments)]
/// Compute scaled style metrics for a style class and expose a stable,
/// allocation-owned representation.
pub fn compute_scaled_style_metrics_exported(
    font: &FontRef,
    coords: &[F2Dot14],
    style: &StyleClass,
    x_scale: i32,
    y_scale: i32,
    x_delta: i32,
    y_delta: i32,
    flags: u32,
    units_per_em: i32,
) -> ExportedScaledStyleMetrics {
    let shaper_mode = if cfg!(feature = "autohint_shaping") {
        ShaperMode::BestEffort
    } else {
        ShaperMode::Nominal
    };
    let shaper = Shaper::new(font, shaper_mode);
    let unscaled = compute_unscaled_style_metrics(&shaper, coords, style);
    let scaled = scale_style_metrics(
        &unscaled,
        Scale {
            x_scale,
            y_scale,
            x_delta,
            y_delta,
            size: 0.0,
            units_per_em,
            flags,
        },
    );

    ExportedScaledStyleMetrics {
        digits_have_same_width: unscaled.digits_have_same_width,
        x_scale: scaled.scale.x_scale,
        y_scale: scaled.scale.y_scale,
        x_delta: scaled.scale.x_delta,
        y_delta: scaled.scale.y_delta,
        horizontal_widths: scaled.axes[0]
            .widths
            .iter()
            .map(|width| ExportedScaledWidth {
                scaled: width.scaled,
                fitted: width.fitted,
            })
            .collect(),
        vertical_widths: scaled.axes[1]
            .widths
            .iter()
            .map(|width| ExportedScaledWidth {
                scaled: width.scaled,
                fitted: width.fitted,
            })
            .collect(),
        blues: scaled.axes[1]
            .blues
            .iter()
            .map(|blue| ExportedScaledBlue {
                reference_scaled: blue.position.scaled,
                reference_fitted: blue.position.fitted,
                shoot_scaled: blue.overshoot.scaled,
                shoot_fitted: blue.overshoot.fitted,
                is_active: blue.is_active,
                is_top: blue.zones.contains(BlueZones::TOP),
                is_sub_top: blue.zones.contains(BlueZones::SUB_TOP),
                is_neutral: blue.zones.contains(BlueZones::NEUTRAL),
                is_adjustment: blue.zones.contains(BlueZones::ADJUSTMENT),
            })
            .collect(),
    }
}

/// Maximum number of widths, same for Latin and CJK.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L65>
/// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afcjk.h#L55>
pub(crate) const MAX_WIDTHS: usize = 16;

/// Unscaled metrics for a single axis.
///
/// This is the union of the Latin and CJK axis records.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L88>
/// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afcjk.h#L73>
#[derive(Clone, Default, Debug)]
pub(crate) struct UnscaledAxisMetrics {
    pub dim: Dimension,
    pub widths: UnscaledWidths,
    pub width_metrics: WidthMetrics,
    pub blues: UnscaledBlues,
}

impl UnscaledAxisMetrics {
    pub fn max_width(&self) -> Option<i32> {
        self.widths.last().copied()
    }
}

/// Scaled metrics for a single axis.
#[derive(Clone, Default, Debug)]
pub(crate) struct ScaledAxisMetrics {
    pub dim: Dimension,
    /// Font unit to 26.6 scale in the axis direction.
    pub scale: i32,
    /// 1/64 pixel delta in the axis direction.
    pub delta: i32,
    pub widths: ScaledWidths,
    pub width_metrics: WidthMetrics,
    pub blues: ScaledBlues,
}

/// Unscaled metrics for a single style and script.
///
/// This is the union of the root, Latin and CJK style metrics but
/// the latter two are actually identical.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aftypes.h#L413>
/// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L109>
/// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afcjk.h#L95>
#[derive(Clone, Default, Debug)]
pub(crate) struct UnscaledStyleMetrics {
    /// Index of style class.
    pub class_ix: u16,
    /// Monospaced digits?
    pub digits_have_same_width: bool,
    /// Per-dimension unscaled metrics.
    pub axes: [UnscaledAxisMetrics; 2],
}

impl UnscaledStyleMetrics {
    pub fn style_class(&self) -> &'static StyleClass {
        &super::style::STYLE_CLASSES[self.class_ix as usize]
    }
}

/// The set of unscaled style metrics for a single font.
///
/// For a variable font, this is dependent on the location in variation space.
#[derive(Clone, Debug)]
pub(crate) enum UnscaledStyleMetricsSet {
    Precomputed(Vec<UnscaledStyleMetrics>),
    #[cfg(feature = "std")]
    Lazy(Arc<RwLock<Vec<Option<UnscaledStyleMetrics>>>>),
}

impl UnscaledStyleMetricsSet {
    /// Creates a precomputed style metrics set containing all metrics
    /// required by the glyph map.
    pub fn precomputed(
        font: &FontRef,
        coords: &[F2Dot14],
        shaper_mode: ShaperMode,
        style_map: &GlyphStyleMap,
    ) -> Self {
        // The metrics_styles() iterator does not report exact size so we
        // preallocate and extend here rather than collect to avoid
        // over allocating memory.
        let shaper = Shaper::new(font, shaper_mode);
        let mut vec = Vec::with_capacity(style_map.metrics_count());
        vec.extend(
            style_map
                .metrics_styles()
                .map(|style| compute_unscaled_style_metrics(&shaper, coords, style)),
        );
        Self::Precomputed(vec)
    }

    /// Creates an unscaled style metrics set where each entry will be
    /// initialized as needed.
    #[cfg(feature = "std")]
    pub fn lazy(style_map: &GlyphStyleMap) -> Self {
        let vec = vec![None; style_map.metrics_count()];
        Self::Lazy(Arc::new(RwLock::new(vec)))
    }

    /// Returns the unscaled style metrics for the given style map and glyph
    /// identifier.
    pub fn get(
        &self,
        font: &FontRef,
        coords: &[F2Dot14],
        shaper_mode: ShaperMode,
        style_map: &GlyphStyleMap,
        glyph_id: GlyphId,
    ) -> Option<UnscaledStyleMetrics> {
        let style = style_map.style(glyph_id)?;
        let index = style_map.metrics_index(style)?;
        match self {
            Self::Precomputed(metrics) => metrics.get(index).cloned(),
            #[cfg(feature = "std")]
            Self::Lazy(lazy) => {
                let read = lazy.read().unwrap();
                let entry = read.get(index)?;
                if let Some(metrics) = &entry {
                    return Some(metrics.clone());
                }
                core::mem::drop(read);
                // The std RwLock doesn't support upgrading and contention is
                // expected to be low, so let's just race to compute the new
                // metrics.
                let shaper = Shaper::new(font, shaper_mode);
                let style_class = style.style_class()?;
                let metrics = compute_unscaled_style_metrics(&shaper, coords, style_class);
                let mut entry = lazy.write().unwrap();
                *entry.get_mut(index)? = Some(metrics.clone());
                Some(metrics)
            }
        }
    }
}

/// Scaled metrics for a single style and script.
#[derive(Clone, Default, Debug)]
pub(crate) struct ScaledStyleMetrics {
    /// Multidimensional scaling factors and deltas.
    pub scale: Scale,
    /// Per-dimension scaled metrics.
    pub axes: [ScaledAxisMetrics; 2],
}

#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub(crate) struct WidthMetrics {
    /// Used for creating edges.
    pub edge_distance_threshold: i32,
    /// Default stem thickness.
    pub standard_width: i32,
    /// Is standard width very light?
    pub is_extra_light: bool,
}

pub(crate) type UnscaledWidths = SmallVec<i32, MAX_WIDTHS>;

#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub(crate) struct ScaledWidth {
    /// Width after applying scale.
    pub scaled: i32,
    /// Grid-fitted width.
    pub fitted: i32,
}

pub(crate) type ScaledWidths = SmallVec<ScaledWidth, MAX_WIDTHS>;

/// Captures scaling parameters which may be modified during metrics
/// computation.
#[derive(Copy, Clone, Default, Debug)]
pub(crate) struct Scale {
    /// Font unit to 26.6 scale in the X direction.
    pub x_scale: i32,
    /// Font unit to 26.6 scale in the Y direction.
    pub y_scale: i32,
    /// In 1/64 device pixels.
    pub x_delta: i32,
    /// In 1/64 device pixels.
    pub y_delta: i32,
    /// Font size in pixels per em.
    pub size: f32,
    /// From the source font.
    pub units_per_em: i32,
    /// Flags that determine hinting functionality.
    pub flags: u32,
}

impl Scale {
    /// Create initial scaling parameters from metrics and hinting target.
    pub fn new(
        size: f32,
        units_per_em: i32,
        font_style: Style,
        target: Target,
        group: ScriptGroup,
    ) -> Self {
        let scale =
            (Fixed::from_bits((size * 64.0) as i32) / Fixed::from_bits(units_per_em)).to_bits();
        let mut flags = 0;
        let is_italic = font_style != Style::Normal;
        let is_mono = target == Target::Mono;
        let is_light = target.is_light() || target.preserve_linear_metrics();
        // Snap vertical stems for monochrome and horizontal LCD rendering.
        if is_mono || target.is_lcd() {
            flags |= Self::HORIZONTAL_SNAP;
        }
        // Snap horizontal stems for monochrome and vertical LCD rendering.
        if is_mono || target.is_vertical_lcd() {
            flags |= Self::VERTICAL_SNAP;
        }
        // Adjust stems to full pixels unless in LCD or light modes.
        if !(target.is_lcd() || is_light) {
            flags |= Self::STEM_ADJUST;
        }
        if is_mono {
            flags |= Self::MONO;
        }
        if group == ScriptGroup::Default {
            // Disable horizontal hinting completely for LCD, light hinting
            // and italic fonts
            // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2674>
            if target.is_lcd() || is_light || is_italic {
                flags |= Self::NO_HORIZONTAL;
            }
        } else {
            // CJK doesn't hint advances
            // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afcjk.c#L1432>
            flags |= Self::NO_ADVANCE;
        }
        // CJK doesn't hint advances
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afcjk.c#L1432>
        if group != ScriptGroup::Default {
            flags |= Self::NO_ADVANCE;
        }
        Self {
            x_scale: scale,
            y_scale: scale,
            x_delta: 0,
            y_delta: 0,
            size,
            units_per_em,
            flags,
        }
    }
}

/// Scaler flags that determine hinting settings.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aftypes.h#L115>
/// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L143>
impl Scale {
    /// Stem width snapping.
    pub const HORIZONTAL_SNAP: u32 = 1 << 0;
    /// Stem height snapping.
    pub const VERTICAL_SNAP: u32 = 1 << 1;
    /// Stem width/height adjustment.
    pub const STEM_ADJUST: u32 = 1 << 2;
    /// Monochrome rendering.
    pub const MONO: u32 = 1 << 3;
    /// Disable horizontal hinting.
    pub const NO_HORIZONTAL: u32 = 1 << 4;
    /// Disable vertical hinting.
    pub const NO_VERTICAL: u32 = 1 << 5;
    /// Disable advance hinting.
    pub const NO_ADVANCE: u32 = 1 << 6;
}

// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L59>
pub(crate) fn sort_and_quantize_widths(widths: &mut UnscaledWidths, threshold: i32) {
    if widths.len() <= 1 {
        return;
    }
    widths.sort_unstable();
    let table = widths.as_mut_slice();
    let mut cur_ix = 0;
    let mut cur_val = table[cur_ix];
    let last_ix = table.len() - 1;
    let mut ix = 1;
    // Compute and use mean values for clusters not larger than
    // `threshold`.
    while ix < table.len() {
        if (table[ix] - cur_val) > threshold || ix == last_ix {
            let mut sum = 0;
            // Fix loop for end of array?
            if (table[ix] - cur_val <= threshold) && ix == last_ix {
                ix += 1;
            }
            for val in &mut table[cur_ix..ix] {
                sum += *val;
                *val = 0;
            }
            table[cur_ix] = sum / ix as i32;
            if ix < last_ix {
                cur_ix = ix + 1;
                cur_val = table[cur_ix];
            }
        }
        ix += 1;
    }
    cur_ix = 1;
    // Compress array to remove zero values
    for ix in 1..table.len() {
        if table[ix] != 0 {
            table[cur_ix] = table[ix];
            cur_ix += 1;
        }
    }
    widths.truncate(cur_ix);
}

// Fixed point helpers
//
// Note: lots of bit fiddling based fixed point math in the autohinter
// so we're opting out of using the strongly typed variants because they
// just add noise and reduce clarity.

pub(crate) fn fixed_mul(a: i32, b: i32) -> i32 {
    (Fixed::from_bits(a) * Fixed::from_bits(b)).to_bits()
}

pub(crate) fn fixed_div(a: i32, b: i32) -> i32 {
    (Fixed::from_bits(a) / Fixed::from_bits(b)).to_bits()
}

pub(crate) fn fixed_mul_div(a: i32, b: i32, c: i32) -> i32 {
    Fixed::from_bits(a)
        .mul_div(Fixed::from_bits(b), Fixed::from_bits(c))
        .to_bits()
}

pub(crate) fn pix_round(a: i32) -> i32 {
    (a + 32) & !63
}

pub(crate) fn pix_floor(a: i32) -> i32 {
    a & !63
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            shape::{Shaper, ShaperMode},
            style::STYLE_CLASSES,
        },
        *,
    };
    use raw::TableProvider;

    #[test]
    fn sort_widths() {
        // We use 10 and 20 as thresholds because the computation used
        // is units_per_em / 100
        assert_eq!(sort_widths_helper(&[1], 10), &[1]);
        assert_eq!(sort_widths_helper(&[1], 20), &[1]);
        assert_eq!(sort_widths_helper(&[60, 20, 40, 35], 10), &[20, 35, 13, 60]);
        assert_eq!(sort_widths_helper(&[60, 20, 40, 35], 20), &[31, 60]);
    }

    fn sort_widths_helper(widths: &[i32], threshold: i32) -> Vec<i32> {
        let mut widths2 = UnscaledWidths::new();
        for width in widths {
            widths2.push(*width);
        }
        sort_and_quantize_widths(&mut widths2, threshold);
        widths2.into_iter().collect()
    }

    #[test]
    fn precomputed_style_set() {
        let font = FontRef::new(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS).unwrap();
        let coords = &[];
        let shaper = Shaper::new(&font, ShaperMode::Nominal);
        let glyph_count = font.maxp().unwrap().num_glyphs() as u32;
        let style_map = GlyphStyleMap::new(glyph_count, &shaper);
        let style_set =
            UnscaledStyleMetricsSet::precomputed(&font, coords, ShaperMode::Nominal, &style_map);
        let UnscaledStyleMetricsSet::Precomputed(set) = &style_set else {
            panic!("we definitely made a precomputed style set");
        };
        // This font has Latin, Hebrew and CJK (for unassigned chars) styles
        assert_eq!(STYLE_CLASSES[set[0].class_ix as usize].name, "Latin");
        assert_eq!(STYLE_CLASSES[set[1].class_ix as usize].name, "Hebrew");
        assert_eq!(
            STYLE_CLASSES[set[2].class_ix as usize].name,
            "CJKV ideographs"
        );
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn lazy_style_set() {
        let font = FontRef::new(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS).unwrap();
        let coords = &[];
        let shaper = Shaper::new(&font, ShaperMode::Nominal);
        let glyph_count = font.maxp().unwrap().num_glyphs() as u32;
        let style_map = GlyphStyleMap::new(glyph_count, &shaper);
        let style_set = UnscaledStyleMetricsSet::lazy(&style_map);
        let all_empty = lazy_set_presence(&style_set);
        // Set starts out all empty
        assert_eq!(all_empty, [false; 3]);
        // First load a CJK glyph
        let metrics2 = style_set
            .get(
                &font,
                coords,
                ShaperMode::Nominal,
                &style_map,
                GlyphId::new(0),
            )
            .unwrap();
        assert_eq!(
            STYLE_CLASSES[metrics2.class_ix as usize].name,
            "CJKV ideographs"
        );
        let only_cjk = lazy_set_presence(&style_set);
        assert_eq!(only_cjk, [false, false, true]);
        // Then a Hebrew glyph
        let metrics1 = style_set
            .get(
                &font,
                coords,
                ShaperMode::Nominal,
                &style_map,
                GlyphId::new(1),
            )
            .unwrap();
        assert_eq!(STYLE_CLASSES[metrics1.class_ix as usize].name, "Hebrew");
        let hebrew_and_cjk = lazy_set_presence(&style_set);
        assert_eq!(hebrew_and_cjk, [false, true, true]);
        // And finally a Latin glyph
        let metrics0 = style_set
            .get(
                &font,
                coords,
                ShaperMode::Nominal,
                &style_map,
                GlyphId::new(15),
            )
            .unwrap();
        assert_eq!(STYLE_CLASSES[metrics0.class_ix as usize].name, "Latin");
        let all_present = lazy_set_presence(&style_set);
        assert_eq!(all_present, [true; 3]);
    }

    fn lazy_set_presence(style_set: &UnscaledStyleMetricsSet) -> Vec<bool> {
        let UnscaledStyleMetricsSet::Lazy(set) = &style_set else {
            panic!("we definitely made a lazy style set");
        };
        set.read()
            .unwrap()
            .iter()
            .map(|opt| opt.is_some())
            .collect()
    }
}
