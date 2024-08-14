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

/// Captures the positions of left and right edges for metrics adjustments.
#[derive(Copy, Clone, Default, Debug)]
pub(crate) struct EdgeMetrics {
    pub left_opos: i32,
    pub left_pos: i32,
    pub right_opos: i32,
    pub right_pos: i32,
}

pub(crate) fn hint_outline(
    outline: &mut Outline,
    metrics: &UnscaledStyleMetrics,
    scale: &Scale,
) -> (i32, Option<EdgeMetrics>) {
    let scaled_metrics = metrics::scale_style_metrics(metrics, *scale);
    let y_scale = scaled_metrics.axes[1].scale;
    let mut axis = Axis::default();
    let hint_top_to_bottom = metrics.style_class().script.hint_top_to_bottom;
    outline.scale(&scaled_metrics.scale);
    let mut edge_metrics = None;
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
            edge_metrics = Some(EdgeMetrics {
                left_pos: left.pos,
                left_opos: left.opos,
                right_pos: right.pos,
                right_opos: right.opos,
            });
        }
    }
    (scaled_metrics.axes[0].scale, edge_metrics)
}
