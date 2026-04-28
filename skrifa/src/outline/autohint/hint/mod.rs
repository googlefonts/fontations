//! Entry point to hinting algorithm.

mod edges;
mod outline;

use super::{
    metrics::{scale_style_metrics, Scale, UnscaledStyleMetrics},
    outline::Outline,
    recorder::HintsRecorder,
    style::{GlyphStyle, ScriptGroup},
    topo::{self, Axis},
};

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
    glyph_style: Option<GlyphStyle>,
) -> HintedMetrics {
    hint_outline_impl(outline, metrics, scale, glyph_style, None).hinted_metrics
}

pub(crate) struct HintedPlan {
    pub hinted_metrics: HintedMetrics,
    #[allow(dead_code)]
    pub vertical_axis: Option<Axis>,
}

#[allow(dead_code)] // used in tests
#[cfg(feature = "autohinter")]
pub(crate) fn hint_outline_with_recorder(
    outline: &mut Outline,
    metrics: &UnscaledStyleMetrics,
    scale: &Scale,
    glyph_style: Option<GlyphStyle>,
    recorder: Option<&mut HintsRecorder>,
) -> HintedMetrics {
    hint_outline_impl(outline, metrics, scale, glyph_style, recorder).hinted_metrics
}

#[cfg(feature = "autohinter")]
pub(crate) fn hint_outline_with_plan(
    outline: &mut Outline,
    metrics: &UnscaledStyleMetrics,
    scale: &Scale,
    glyph_style: Option<GlyphStyle>,
    recorder: Option<&mut HintsRecorder>,
) -> HintedPlan {
    hint_outline_impl(outline, metrics, scale, glyph_style, recorder)
}

fn hint_outline_impl(
    outline: &mut Outline,
    metrics: &UnscaledStyleMetrics,
    scale: &Scale,
    glyph_style: Option<GlyphStyle>,
    mut recorder: Option<&mut HintsRecorder>,
) -> HintedPlan {
    let scaled_metrics = scale_style_metrics(metrics, *scale);
    let scale = &scaled_metrics.scale;
    let mut axis = Axis::default();
    let hint_top_to_bottom = metrics.style_class().script.hint_top_to_bottom;
    outline.scale(&scaled_metrics.scale);
    let mut hinted_metrics = HintedMetrics {
        x_scale: scale.x_scale,
        ..Default::default()
    };
    let group = metrics.style_class().script.group;
    let mut vertical_axis = None;
    // For default script group, we don't proceed with hinting if we're
    // missing alignment zones. FreeType swaps in a "dummy" hinter here
    // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afglobal.c#L475>
    if group == ScriptGroup::Default && scaled_metrics.axes[1].blues.is_empty() {
        return HintedPlan {
            hinted_metrics,
            vertical_axis,
        };
    }
    for dim in 0..2 {
        if (dim == Axis::HORIZONTAL && scale.flags & Scale::NO_HORIZONTAL != 0)
            || (dim == Axis::VERTICAL && scale.flags & Scale::NO_VERTICAL != 0)
        {
            continue;
        }
        axis.reset(dim, outline.orientation);
        topo::compute_segments(outline, &mut axis, group);
        topo::link_segments(
            outline,
            &mut axis,
            scaled_metrics.axes[dim].scale,
            group,
            metrics.axes[dim].max_width(),
        );
        topo::compute_edges(
            &mut axis,
            &scaled_metrics.axes[dim],
            hint_top_to_bottom,
            scaled_metrics.scale.y_scale,
            group,
        );
        if dim == Axis::VERTICAL {
            if group != ScriptGroup::Default
                || glyph_style
                    .map(|style| !style.is_non_base())
                    .unwrap_or(true)
            {
                topo::compute_blue_edges(
                    &mut axis,
                    scale,
                    &metrics.axes[dim].blues,
                    &scaled_metrics.axes[dim].blues,
                    group,
                );
            }
        } else {
            hinted_metrics.x_scale = scaled_metrics.axes[0].scale;
        }
        edges::hint_edges(
            &mut axis,
            &scaled_metrics.axes[dim],
            group,
            scale,
            hint_top_to_bottom,
            recorder.as_deref_mut(),
        );
        outline::align_edge_points(outline, &axis, group, scale);
        outline::align_strong_points(outline, &mut axis, recorder.as_deref_mut());
        outline::align_weak_points(outline, dim);
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
        if dim == Axis::VERTICAL {
            vertical_axis = Some(axis.clone());
        }
    }
    HintedPlan {
        hinted_metrics,
        vertical_axis,
    }
}
