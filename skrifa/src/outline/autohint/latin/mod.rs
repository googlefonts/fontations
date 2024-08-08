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

pub(crate) fn hint_outline(outline: &mut Outline, metrics: &UnscaledStyleMetrics, scale: &Scale) {
    let scaled_metrics = metrics::scale_style_metrics(metrics, *scale);
    let mut axis = Axis::default();
    let hint_top_to_bottom =
        super::style::SCRIPT_CLASSES[metrics.class_ix as usize].hint_top_to_bottom;
    outline.scale(&scaled_metrics.scale);
    for dim in 0..2 {
        axis.reset(dim, outline.orientation);
        segments::compute_segments(outline, &mut axis);
        segments::link_segments(outline, &mut axis, metrics.axes[dim].max_width());
        edges::compute_edges(&mut axis, &scaled_metrics.axes[dim], hint_top_to_bottom);
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
    }
}
