//! Autohinting specific metrics.

use super::axis::Dimension;
use crate::collections::SmallVec;
use raw::types::Fixed;

/// Maximum number of widths, same for Latin and CJK.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L65>
/// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afcjk.h#L55>
pub(crate) const MAX_WIDTHS: usize = 16;

/// Maximum number of blue values.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afblue.h#L328>
pub(crate) const MAX_BLUES: usize = 8;

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
    /// Monospaced digits?
    pub digits_have_same_width: bool,
    /// Per-dimension unscaled metrics.
    pub axes: [UnscaledAxisMetrics; 2],
}

/// Scaled metrics for a single style and script.
#[derive(Clone, Default, Debug)]
pub(crate) struct ScaledStyleMetrics {
    /// Multidimensional scaling factors and deltas.
    pub scale: Scale,
    /// Control flags to partially disable hinting.
    pub flags: u16,
    /// Per-dimension scaled metrics.
    pub axes: [ScaledAxisMetrics; 2],
}

/// Scaled metrics flags.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aftypes.h#L115>
impl ScaledStyleMetrics {
    /// Disable horizontal hinting.
    pub(crate) const NO_HORIZONTAL: u16 = 1;
    /// Disable vertical hinting.
    pub(crate) const NO_VERTICAL: u16 = 2;
    /// Disable advance hinting.
    pub(crate) const NO_ADVANCE: u16 = 4;
}

// FreeType keeps a single array of blue values per metrics set
// and mutates when the scale factor changes. We'll separate them so
// that we can reuse unscaled metrics as immutable state without
// recomputing them (which is the expensive part).
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L77>
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub(crate) struct UnscaledBlue {
    pub position: i32,
    pub overshoot: i32,
    pub ascender: i32,
    pub descender: i32,
    pub flags: u32,
}

pub(crate) type UnscaledBlues = SmallVec<UnscaledBlue, MAX_BLUES>;

#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct ScaledBlue {
    pub position: ScaledWidth,
    pub overshoot: ScaledWidth,
    pub flags: u32,
}

pub(crate) type ScaledBlues = SmallVec<ScaledBlue, MAX_BLUES>;

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
    /// From the source font.
    pub units_per_em: i32,
}

impl Scale {
    /// Create initial scaling parameters from font size and units per em.
    pub fn new(size: f32, units_per_em: i32) -> Self {
        let scale =
            (Fixed::from_bits((size * 64.0) as i32) / Fixed::from_bits(units_per_em)).to_bits();
        Self {
            x_scale: scale,
            y_scale: scale,
            x_delta: 0,
            y_delta: 0,
            units_per_em,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
