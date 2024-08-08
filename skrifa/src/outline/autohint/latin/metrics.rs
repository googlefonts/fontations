//! Latin metrics computation.
//!
//! Uses the widths and blues computations to generate unscaled metrics for a
//! given style/script.
//!
//! Then applies a scaling factor to those metrics, computes a potentially
//! modified scale, and tags active blue zones.

use super::super::{
    axis::{Axis, Dimension},
    metrics::{
        fixed_mul, fixed_mul_div, pix_round, Scale, ScaledAxisMetrics, ScaledBlue,
        ScaledStyleMetrics, ScaledWidth, UnscaledAxisMetrics, UnscaledBlue, UnscaledBlues,
        UnscaledStyleMetrics, WidthMetrics,
    },
    style::{blue_flags, ScriptClass},
};
use crate::{prelude::Size, MetadataProvider};
use raw::{types::F2Dot14, FontRef};

/// Computes unscaled metrics for the Latin writing system.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L1134>
pub(crate) fn compute_unscaled_style_metrics(
    font: &FontRef,
    coords: &[F2Dot14],
    style: &ScriptClass,
) -> UnscaledStyleMetrics {
    let [hwidths, vwidths] = super::widths::compute_widths(font, coords, style);
    let blues = UnscaledBlues::new_latin(font, coords, style);
    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(Size::unscaled(), coords);
    let mut digit_advance = None;
    let mut digits_have_same_width = true;
    for ch in '0'..='9' {
        if let Some(advance) = charmap
            .map(ch)
            .and_then(|gid| glyph_metrics.advance_width(gid))
        {
            if digit_advance.is_some() && digit_advance != Some(advance) {
                digits_have_same_width = false;
                break;
            }
            digit_advance = Some(advance);
        }
    }
    UnscaledStyleMetrics {
        class_ix: style.index as u16,
        digits_have_same_width,
        axes: [
            UnscaledAxisMetrics {
                dim: Axis::HORIZONTAL,
                // Latin doesn't have horizontal blues
                blues: Default::default(),
                width_metrics: hwidths.0,
                widths: hwidths.1,
            },
            UnscaledAxisMetrics {
                dim: Axis::VERTICAL,
                blues,
                width_metrics: vwidths.0,
                widths: vwidths.1,
            },
        ],
    }
}

/// Computes scaled metrics for the Latin writing system.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L1491>
pub(crate) fn scale_style_metrics(
    unscaled_metrics: &UnscaledStyleMetrics,
    mut scale: Scale,
) -> ScaledStyleMetrics {
    let mut scale_axis = |axis: &UnscaledAxisMetrics| {
        scale_axis_metrics(
            axis.dim,
            &axis.widths,
            axis.width_metrics,
            &axis.blues,
            &mut scale,
        )
    };
    let axes = [
        scale_axis(&unscaled_metrics.axes[0]),
        scale_axis(&unscaled_metrics.axes[1]),
    ];
    ScaledStyleMetrics { scale, axes }
}

/// Computes scaled metrics for a single axis.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L1168>
fn scale_axis_metrics(
    dim: Dimension,
    widths: &[i32],
    width_metrics: WidthMetrics,
    blues: &[UnscaledBlue],
    scale: &mut Scale,
) -> ScaledAxisMetrics {
    let mut axis = ScaledAxisMetrics::default();
    if dim == Axis::HORIZONTAL {
        axis.scale = scale.x_scale;
        axis.delta = scale.x_delta;
    } else {
        axis.scale = scale.y_scale;
        axis.delta = scale.y_delta;
    };
    // Correct Y scale to optimize alignment
    if let Some(blue_ix) = blues
        .iter()
        .position(|blue| blue.flags & blue_flags::LATIN_BLUE_ADJUSTMENT != 0)
    {
        let unscaled_blue = &blues[blue_ix];
        let scaled = fixed_mul(axis.scale, unscaled_blue.overshoot);
        let fitted = (scaled + 40) & !63;
        if scaled != fitted && dim == Axis::VERTICAL {
            let new_scale = fixed_mul_div(axis.scale, fitted, scaled);
            // Scaling should not adjust by more than 2 pixels
            let mut max_height = scale.units_per_em;
            for blue in blues {
                max_height = max_height.max(blue.ascender).max(-blue.descender);
            }
            let mut dist = fixed_mul(max_height, new_scale - axis.scale);
            dist &= !127;
            if dist == 0 {
                axis.scale = new_scale;
                scale.y_scale = new_scale;
            }
        }
    }
    // Now scale the widths
    axis.width_metrics = width_metrics;
    for unscaled_width in widths {
        let scaled = fixed_mul(axis.scale, *unscaled_width);
        axis.widths.push(ScaledWidth {
            scaled,
            fitted: scaled,
        });
    }
    // Compute extra light property: this is a standard width that is
    // less than 5/8 pixels
    axis.width_metrics.is_extra_light =
        fixed_mul(axis.width_metrics.standard_width, axis.scale) < (32 + 8);
    if dim == Axis::VERTICAL {
        // And scale the blue zones
        for unscaled_blue in blues {
            let scaled_position = fixed_mul(axis.scale, unscaled_blue.position) + axis.delta;
            let scaled_overshoot = fixed_mul(axis.scale, unscaled_blue.overshoot) + axis.delta;
            let mut blue = ScaledBlue {
                position: ScaledWidth {
                    scaled: scaled_position,
                    fitted: scaled_position,
                },
                overshoot: ScaledWidth {
                    scaled: scaled_overshoot,
                    fitted: scaled_overshoot,
                },
                flags: unscaled_blue.flags & !blue_flags::LATIN_ACTIVE,
            };
            // Only activate blue zones less than 3/4 pixel tall
            let dist = fixed_mul(unscaled_blue.position - unscaled_blue.overshoot, axis.scale);
            if (-48..=48).contains(&dist) {
                let mut delta = dist.abs();
                if delta < 32 {
                    delta = 0;
                } else if delta < 48 {
                    delta = 32;
                } else {
                    delta = 64;
                }
                if dist < 0 {
                    delta = -delta;
                }
                blue.position.fitted = pix_round(blue.position.scaled);
                blue.overshoot.fitted = blue.position.fitted - delta;
                blue.flags |= blue_flags::LATIN_ACTIVE;
            }
            axis.blues.push(blue);
        }
        // Use sub-top blue zone if it doesn't overlap with another
        // non-sub-top blue zone
        for blue_ix in 0..axis.blues.len() {
            const REQUIRED_FLAGS: u32 = blue_flags::LATIN_SUB_TOP | blue_flags::LATIN_ACTIVE;
            let blue = axis.blues[blue_ix];
            if blue.flags & REQUIRED_FLAGS != REQUIRED_FLAGS {
                continue;
            }
            for blue_ix2 in 0..axis.blues.len() {
                let blue2 = axis.blues[blue_ix2];
                if blue2.flags & blue_flags::LATIN_SUB_TOP != 0 {
                    continue;
                }
                if blue2.flags & blue_flags::LATIN_ACTIVE == 0 {
                    continue;
                }
                if blue2.position.fitted <= blue.overshoot.fitted
                    && blue2.overshoot.fitted >= blue.position.fitted
                {
                    axis.blues[blue_ix].flags &= !blue_flags::LATIN_ACTIVE;
                    break;
                }
            }
        }
    }
    axis
}

#[cfg(test)]
mod tests {
    use super::{super::super::style, *};
    use raw::{FontRef, TableProvider};

    #[test]
    fn scaled_metrics() {
        let font = FontRef::new(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS).unwrap();
        let class = &style::SCRIPT_CLASSES[ScriptClass::HEBR];
        let unscaled_metrics = compute_unscaled_style_metrics(&font, Default::default(), class);
        let scale = Scale::new(
            16.0,
            font.head().unwrap().units_per_em() as i32,
            Default::default(),
        );
        let scaled_metrics = scale_style_metrics(&unscaled_metrics, scale);
        // Check scale and deltas
        assert_eq!(scaled_metrics.scale.x_scale, 67109);
        assert_eq!(scaled_metrics.scale.y_scale, 67109);
        assert_eq!(scaled_metrics.scale.x_delta, 0);
        assert_eq!(scaled_metrics.scale.y_delta, 0);
        // Horizontal widths
        let h_axis = &scaled_metrics.axes[0];
        let expected_h_widths = [55];
        let h_widths = h_axis
            .widths
            .iter()
            .map(|width| width.scaled)
            .collect::<Vec<_>>();
        assert_eq!(h_widths, expected_h_widths);
        // Latin never has horizontal blues
        assert!(h_axis.blues.is_empty());
        // Not extra light
        assert!(!h_axis.width_metrics.is_extra_light);
        // Vertical widths
        let v_axis = &scaled_metrics.axes[1];
        let expected_v_widths = [22, 112];
        let v_widths = v_axis
            .widths
            .iter()
            .map(|width| width.scaled)
            .collect::<Vec<_>>();
        assert_eq!(v_widths, expected_v_widths);
        // Vertical blues
        #[rustfmt::skip]
        let expected_v_blues = [
            // ((scaled_pos, fitted_pos), (scaled_shoot, fitted_shoot), flags)
            ((606, 576), (606, 576), blue_flags::LATIN_ACTIVE | blue_flags::TOP),
            ((0, 0), (-9, 0), blue_flags::LATIN_ACTIVE),
            ((-246, -256), (-246, -256), blue_flags::LATIN_ACTIVE),
        ];
        let v_blues = v_axis
            .blues
            .iter()
            .map(|blue| {
                (
                    (blue.position.scaled, blue.position.fitted),
                    (blue.overshoot.scaled, blue.overshoot.fitted),
                    blue.flags,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(v_blues, expected_v_blues);
        // This one is extra light
        assert!(v_axis.width_metrics.is_extra_light);
    }
}
