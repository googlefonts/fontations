//! Latin writing system.

mod blues;
mod edges;
mod hint;
mod metrics;
mod segments;
mod widths;

use super::{
    axis::Axis,
    metrics::{Scale, UnscaledStyleMetrics},
    outline::Outline,
};

pub(crate) use metrics::compute_unscaled_style_metrics;

/// All constants are defined based on a UPEM of 2048.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L34>
fn derived_constant(units_per_em: i32, value: i32) -> i32 {
    value * units_per_em / 2048
}

/// Captures adjusted horizontal scale and outer edge positions to be used
/// for horizontal metrics adjustments.
#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub(crate) struct EdgeMetrics {
    pub left_opos: i32,
    pub left_pos: i32,
    pub right_opos: i32,
    pub right_pos: i32,
}

#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub(crate) struct HintedMetrics {
    pub x_scale: i32,
    /// This will be `None` when we've identified fewer than two horizontal
    /// edges in the outline. This will occur for empty outlines and outlines
    /// that are degenerate (all x coordinates have the same value, within
    /// a threshold). In these cases, horizontal metrics will not be adjusted.
    pub edge_metrics: Option<EdgeMetrics>,
}

/// Applies the complete hinting process to a latin outline.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3554>
pub(crate) fn hint_outline(
    outline: &mut Outline,
    metrics: &UnscaledStyleMetrics,
    scale: &Scale,
) -> HintedMetrics {
    let scaled_metrics = metrics::scale_style_metrics(metrics, *scale);
    let y_scale = scaled_metrics.axes[1].scale;
    let mut axis = Axis::default();
    let hint_top_to_bottom = metrics.style_class().script.hint_top_to_bottom;
    outline.scale(&scaled_metrics.scale);
    let mut hinted_metrics = HintedMetrics::default();
    for dim in 0..2 {
        axis.reset(dim, outline.orientation);
        segments::compute_segments(outline, &mut axis);
        segments::link_segments(outline, &mut axis, metrics.axes[dim].max_width());
        edges::compute_edges(
            &mut axis,
            &scaled_metrics.axes[dim],
            hint_top_to_bottom,
            scaled_metrics.scale.y_scale,
        );
        if dim == Axis::VERTICAL {
            edges::compute_blue_edges(
                &mut axis,
                scale,
                &metrics.axes[dim].blues,
                &scaled_metrics.axes[dim].blues,
            );
        } else {
            hinted_metrics.x_scale = scaled_metrics.axes[0].scale;
        }
        hint::hint_edges(
            &mut axis,
            &scaled_metrics.axes[dim],
            scale,
            hint_top_to_bottom,
        );
        super::hint::align_edge_points(outline, &axis);
        super::hint::align_strong_points(outline, &mut axis);
        super::hint::align_weak_points(outline, dim);
        if dim == 0 && axis.edges.len() > 1 {
            let left = axis.edges.first().unwrap();
            let right = axis.edges.last().unwrap();
            hinted_metrics.edge_metrics = Some(EdgeMetrics {
                left_pos: left.pos,
                left_opos: left.opos,
                right_pos: right.pos,
                right_opos: right.opos,
            });
        }
    }
    hinted_metrics
}
