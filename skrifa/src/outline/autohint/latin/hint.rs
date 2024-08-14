//! Latin edge hinting.
//!
//! Let's actually do some grid fitting. Here we align edges to the pixel
//! grid. This is the final step before applying the edge adjustments to
//! the original outline points.

use super::super::{
    axis::{Axis, Dimension, Edge},
    metrics::{fixed_mul_div, pix_round, Scale, ScaledAxisMetrics, ScaledWidth},
};

/// Main Latin grid-fitting routine.
///
/// Note: this is one huge function in FreeType, broken up into several below.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2999>
pub(crate) fn hint_edges(
    axis: &mut Axis,
    metrics: &ScaledAxisMetrics,
    scale: &Scale,
    mut top_to_bottom_hinting: bool,
) {
    if axis.dim != Axis::VERTICAL {
        top_to_bottom_hinting = false;
    }
    // First align horizontal edges to blue zones if needed
    let anchor_ix = align_edges_to_blues(axis, metrics, scale);
    // Now align the stem edges
    let (serif_count, anchor_ix) =
        align_stem_edges(axis, metrics, scale, top_to_bottom_hinting, anchor_ix);
    let edges = axis.edges.as_mut_slice();
    // Special case for lowercase m
    if axis.dim == Axis::HORIZONTAL && (edges.len() == 6 || edges.len() == 12) {
        hint_lowercase_m(edges);
    }
    // Handle serifs and single segment edges
    if serif_count > 0 || anchor_ix.is_none() {
        align_remaining_edges(axis, top_to_bottom_hinting, anchor_ix);
    }
}

/// Align horizontal edges to blue zones.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3030>
fn align_edges_to_blues(
    axis: &mut Axis,
    metrics: &ScaledAxisMetrics,
    scale: &Scale,
) -> Option<usize> {
    let mut anchor_ix = None;
    // For a vertical axis, begin by aligning stems to blue zones
    if axis.dim == Axis::VERTICAL {
        for edge_ix in 0..axis.edges.len() {
            let edges = axis.edges.as_mut_slice();
            let edge = &edges[edge_ix];
            if edge.flags & Edge::DONE != 0 {
                continue;
            }
            let edge2_ix = edge.link_ix.map(|x| x as usize);
            let edge2 = edge2_ix.map(|ix| &edges[ix]);
            // If we have two neutral zones, skip one of them
            if let (true, Some(edge2)) = (edge.blue_edge.is_some(), edge2) {
                if edge2.blue_edge.is_some() {
                    let skip_ix = if edge2.flags & Edge::NEUTRAL != 0 {
                        edge2_ix
                    } else if edge.flags & Edge::NEUTRAL != 0 {
                        Some(edge_ix)
                    } else {
                        None
                    };
                    if let Some(skip_ix) = skip_ix {
                        let skip_edge = &mut edges[skip_ix];
                        skip_edge.blue_edge = None;
                        skip_edge.flags &= !Edge::NEUTRAL;
                    }
                }
            }
            // Flip edges if the other is aligned to a blue zone
            let blue = edges[edge_ix].blue_edge;
            let (blue, edge1_ix, edge2_ix) = if let Some(blue) = blue {
                (blue, Some(edge_ix), edge2_ix)
            } else if let Some(edge2_blue) = edge2_ix.and_then(|ix| edges[ix].blue_edge) {
                (edge2_blue, edge2_ix, Some(edge_ix))
            } else {
                (Default::default(), None, None)
            };
            let Some(edge1_ix) = edge1_ix else {
                continue;
            };
            let edge1 = &mut edges[edge1_ix];
            edge1.pos = blue.fitted;
            edge1.flags |= Edge::DONE;
            if let Some(edge2_ix) = edge2_ix {
                let edge2 = &mut edges[edge2_ix];
                if edge2.blue_edge.is_none() {
                    edge2.flags |= Edge::DONE;
                    align_linked_edge(axis, metrics, scale, edge1_ix, edge2_ix);
                }
            }
            if anchor_ix.is_none() {
                anchor_ix = Some(edge_ix);
            }
        }
    }
    anchor_ix
}

/// Align stem edges, trying to main relative order of stems in the glyph.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3123>
fn align_stem_edges(
    axis: &mut Axis,
    metrics: &ScaledAxisMetrics,
    scale: &Scale,
    top_to_bottom_hinting: bool,
    mut anchor_ix: Option<usize>,
) -> (usize, Option<usize>) {
    let mut serif_count = 0;
    // Now align all other stem edges
    // This code starts at: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3123>
    for edge_ix in 0..axis.edges.len() {
        let edges = axis.edges.as_mut_slice();
        let edge = &edges[edge_ix];
        if edge.flags & Edge::DONE != 0 {
            continue;
        }
        // Skip all non-stem edges
        let Some(edge2_ix) = edge.link_ix.map(|ix| ix as usize) else {
            serif_count += 1;
            continue;
        };
        // This shouldn't happen?
        if edges[edge2_ix].blue_edge.is_some() {
            edges[edge2_ix].flags |= Edge::DONE;
            align_linked_edge(axis, metrics, scale, edge2_ix, edge_ix);
            continue;
        }
        // Now align the stem
        // Note: the branches here are reversed from the FreeType code
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3155>
        if let Some(anchor_ix) = anchor_ix {
            let anchor = &edges[anchor_ix];
            let edge = edges[edge_ix];
            let edge2 = edges[edge2_ix];
            let original_pos = anchor.pos + (edge.opos - anchor.opos);
            let original_len = edge2.opos - edge.opos;
            let original_center = original_pos + (original_len >> 1);
            let cur_len = stem_width(
                axis.dim,
                metrics,
                scale,
                original_len,
                0,
                edge.flags,
                edge2.flags,
            );
            if edge2.flags & Edge::DONE != 0 {
                let new_pos = edge2.pos - cur_len;
                edges[edge_ix].pos = new_pos;
            } else if cur_len < 96 {
                let cur_pos1 = pix_round(original_center);
                let (u_off, d_off) = if cur_len <= 64 { (32, 32) } else { (38, 26) };
                let delta1 = (original_center - (cur_pos1 - u_off)).abs();
                let delta2 = (original_center - (cur_pos1 + d_off)).abs();
                let cur_pos1 = if delta1 < delta2 {
                    cur_pos1 - u_off
                } else {
                    cur_pos1 + d_off
                };
                edges[edge_ix].pos = cur_pos1 - cur_len / 2;
                edges[edge2_ix].pos = cur_pos1 + cur_len / 2;
            } else {
                let cur_pos1 = pix_round(original_pos);
                let delta1 = (cur_pos1 + (cur_len >> 1) - original_center).abs();
                let cur_pos2 = pix_round(original_pos + original_len) - cur_len;
                let delta2 = (cur_pos2 + (cur_len >> 1) - original_center).abs();
                let new_pos = if delta1 < delta2 { cur_pos1 } else { cur_pos2 };
                let new_pos2 = new_pos + cur_len;
                edges[edge_ix].pos = new_pos;
                edges[edge2_ix].pos = new_pos2;
            }
            edges[edge_ix].flags |= Edge::DONE;
            edges[edge2_ix].flags |= Edge::DONE;
            if edge_ix > 0 {
                adjust_link(edges, edge_ix, LinkDir::Prev, top_to_bottom_hinting);
            }
        } else {
            // No stem has been aligned yet
            let edge = edges[edge_ix];
            let edge2 = edges[edge2_ix];
            let original_len = edge2.opos - edge.opos;
            let cur_len = stem_width(
                axis.dim,
                metrics,
                scale,
                original_len,
                0,
                edge.flags,
                edge2.flags,
            );
            // Some "voodoo" to specially round edges for small stem widths
            let (u_off, d_off) = if cur_len <= 64 {
                // width <= 1px
                (32, 32)
            } else {
                // 1px < width < 1.5px
                (38, 26)
            };
            if cur_len < 96 {
                let original_center = edge.opos + (original_len >> 1);
                let mut cur_pos1 = pix_round(original_center);
                let error1 = (original_center - (cur_pos1 - u_off)).abs();
                let error2 = (original_center - (cur_pos1 + d_off)).abs();
                if error1 < error2 {
                    cur_pos1 -= u_off;
                } else {
                    cur_pos1 += d_off;
                }
                let edge_pos = cur_pos1 - cur_len / 2;
                edges[edge_ix].pos = edge_pos;
                edges[edge2_ix].pos = edge_pos + cur_len;
            } else {
                edges[edge_ix].pos = pix_round(edge.opos);
            }
            edges[edge_ix].flags |= Edge::DONE;
            align_linked_edge(axis, metrics, scale, edge_ix, edge2_ix);
            anchor_ix = Some(edge_ix);
        }
    }
    (serif_count, anchor_ix)
}

/// Make sure that lowercase m's maintain symmetry.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3365>
fn hint_lowercase_m(edges: &mut [Edge]) {
    let (edge1_ix, edge2_ix, edge3_ix) = if edges.len() == 6 {
        (0, 2, 4)
    } else {
        (1, 5, 9)
    };
    let edge1 = &edges[edge1_ix];
    let edge2 = &edges[edge2_ix];
    let edge3 = &edges[edge3_ix];
    let dist1 = edge2.opos - edge1.opos;
    let dist2 = edge3.opos - edge2.opos;
    let span = (dist1 - dist2).abs();
    if span < 8 {
        let delta = edge3.pos - (2 * edge2.pos - edge1.pos);
        let link_ix = edge3.link_ix.map(|ix| ix as usize);
        let edge3 = &mut edges[edge3_ix];
        edge3.pos -= delta;
        edge3.flags |= Edge::DONE;
        if let Some(link_ix) = link_ix {
            let link = &mut edges[link_ix];
            link.pos -= delta;
            link.flags |= Edge::DONE;
        }
        // Move serifs along with the stem
        if edges.len() == 12 {
            edges[8].pos -= delta;
            edges[11].pos -= delta;
        }
    }
}

/// Align serif and single segment edges.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3418>
fn align_remaining_edges(
    axis: &mut Axis,
    top_to_bottom_hinting: bool,
    mut anchor_ix: Option<usize>,
) {
    for edge_ix in 0..axis.edges.len() {
        let edges = &mut axis.edges;
        let edge = &edges[edge_ix];
        if edge.flags & Edge::DONE != 0 {
            continue;
        }
        let mut delta = 1000;
        if let Some(serif) = edge.serif(edges) {
            delta = (serif.opos - edge.opos).abs();
        }
        if delta < 64 + 16 {
            // delta is only < 1000 if edge.serif_ix is Some(_)
            let serif_ix = edge.serif_ix.unwrap() as usize;
            align_serif_edge(axis, serif_ix, edge_ix)
        } else if let Some(anchor_ix) = anchor_ix {
            let mut before_ix = None;
            for ix in (0..=edge_ix.saturating_sub(1)).rev() {
                if edges[ix].flags & Edge::DONE != 0 {
                    before_ix = Some(ix);
                    break;
                }
            }
            let mut after_ix = None;
            for ix in edge_ix + 1..edges.len() {
                if edges[ix].flags & Edge::DONE != 0 {
                    after_ix = Some(ix);
                    break;
                }
            }
            if let Some((before_ix, after_ix)) = before_ix.zip(after_ix) {
                let before = &edges[before_ix];
                let after = &edges[after_ix];
                let new_pos = if after.opos == before.opos {
                    before.pos
                } else {
                    before.pos
                        + fixed_mul_div(
                            edge.opos - before.opos,
                            after.pos - before.pos,
                            after.opos - before.opos,
                        )
                };
                edges[edge_ix].pos = new_pos;
            } else {
                let anchor = &edges[anchor_ix];
                let new_pos = anchor.pos + ((edge.opos - anchor.opos + 16) & !31);
                edges[edge_ix].pos = new_pos;
            }
        } else {
            anchor_ix = Some(edge_ix);
            let new_pos = pix_round(edge.opos);
            edges[edge_ix].pos = new_pos;
        }
        let edges = &mut axis.edges;
        edges[edge_ix].flags |= Edge::DONE;
        adjust_link(edges, edge_ix, LinkDir::Prev, top_to_bottom_hinting);
        adjust_link(edges, edge_ix, LinkDir::Next, top_to_bottom_hinting);
    }
}

#[derive(Copy, Clone, PartialEq)]
enum LinkDir {
    Prev,
    Next,
}

/// Helper to adjust links based on hinting direction.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L3499>
fn adjust_link(
    edges: &mut [Edge],
    edge_ix: usize,
    link_dir: LinkDir,
    top_to_bottom_hinting: bool,
) -> Option<()> {
    let edge = &edges[edge_ix];
    let (edge2, prev_edge) = if link_dir == LinkDir::Next {
        let edge2 = edges.get(edge_ix + 1)?;
        // Don't adjust next edge if it's not done yet
        if edge2.flags & Edge::DONE == 0 {
            return None;
        }
        (edge2, edges.get(edge_ix.checked_sub(1)?)?)
    } else {
        let edge = edges.get(edge_ix.checked_sub(1)?)?;
        (edge, edge)
    };
    let pos1 = edge.pos;
    let pos2 = edge2.pos;
    let order_check = match (link_dir, top_to_bottom_hinting) {
        (LinkDir::Prev, true) | (LinkDir::Next, false) => pos1 > pos2,
        (LinkDir::Prev, false) | (LinkDir::Next, true) => pos1 < pos2,
    };
    if !order_check {
        return None;
    }
    let link = edge.link(edges)?;
    if (link.pos - prev_edge.pos).abs() > 16 {
        let new_pos = edge2.pos;
        edges[edge_ix].pos = new_pos;
    }
    Some(())
}

/// Snap a scaled width to one of the standard widths.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2697>
fn snap_width(widths: &[ScaledWidth], width: i32) -> i32 {
    let (_, ref_width) =
        widths
            .iter()
            .fold((64 + 32 + 2, width), |(best_dist, ref_width), candidate| {
                let dist = (width - candidate.scaled).abs();
                if dist < best_dist {
                    (dist, candidate.scaled)
                } else {
                    (best_dist, ref_width)
                }
            });
    let scaled = pix_round(ref_width);
    if width >= ref_width {
        if width < scaled + 48 {
            ref_width
        } else {
            width
        }
    } else if width > scaled - 48 {
        ref_width
    } else {
        width
    }
}

/// Compute the snapped width of a given stem.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2746>
fn stem_width(
    dim: Dimension,
    metrics: &ScaledAxisMetrics,
    scale: &Scale,
    width: i32,
    base_delta: i32,
    base_flags: u8,
    stem_flags: u8,
) -> i32 {
    if scale.flags & Scale::STEM_ADJUST == 0 || metrics.width_metrics.is_extra_light {
        return width;
    }
    let is_vertical = dim == Axis::VERTICAL;
    let sign = if width < 0 { -1 } else { 1 };
    let mut dist = width.abs();
    if (is_vertical && scale.flags & Scale::VERTICAL_SNAP == 0)
        || (!is_vertical && scale.flags & Scale::HORIZONTAL_SNAP == 0)
    {
        // Do smooth hinting
        if (stem_flags & Edge::SERIF != 0) && is_vertical && (dist < 3 * 64) {
            // Don't touch widths of serifs
            return dist * sign;
        } else if base_flags & Edge::ROUND != 0 {
            if dist < 80 {
                dist = 64;
            }
        } else if dist < 56 {
            dist = 56;
        }
        if !metrics.widths.is_empty() {
            // Compare to standard width
            let min_width = metrics.widths[0].scaled;
            let delta = (dist - min_width).abs();
            if delta < 40 {
                dist = min_width.max(48);
                return dist * sign;
            }
            if dist < 3 * 64 {
                let delta = dist & 63;
                dist &= -64;
                if delta < 10 {
                    dist += delta;
                } else if delta < 32 {
                    dist += 10;
                } else if delta < 54 {
                    dist += 54;
                } else {
                    dist += delta;
                }
            } else {
                let mut new_base_delta = 0;
                if (width > 0 && base_delta > 0) || (width < 0 && base_delta < 0) {
                    if scale.size < 10.0 {
                        new_base_delta = base_delta;
                    } else if scale.size < 30.0 {
                        new_base_delta = (base_delta * (30.0 - scale.size) as i32) / 20;
                    }
                }
                dist = (dist - new_base_delta.abs() + 32) & !63;
            }
        }
    } else {
        // Do strong hinting: snap to integer pixels
        let original_dist = dist;
        dist = snap_width(&metrics.widths, dist);
        if is_vertical {
            // Always round to integers in the vertical case
            if dist >= 64 {
                dist = (dist + 16) & !63;
            } else {
                dist = 64;
            }
        } else if scale.flags & Scale::MONO != 0 {
            // Mono horizontal hinting: snap to integer with different
            // threshold
            if dist < 64 {
                dist = 64;
            } else {
                dist = (dist + 32) & !63;
            }
        } else {
            // Smooth horizontal hinting: strengthen small stems, round
            // stems whose size is between 1 and 2 pixels
            if dist < 48 {
                dist = (dist + 64) >> 1;
            } else if dist < 128 {
                // Only round to integer if distortion is less than
                // 1/4 pixel
                dist = (dist + 22) & !63;
                let delta = (dist - original_dist).abs();
                if delta >= 16 {
                    dist = original_dist;
                    if dist < 48 {
                        dist = (dist + 64) >> 1;
                    }
                }
            } else {
                // Round otherwise to prevent color fringes in LCD mode
                dist = (dist + 32) & !63;
            }
        }
    }
    dist * sign
}

/// Align one stem edge relative to previous stem edge.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2943>
fn align_linked_edge(
    axis: &mut Axis,
    metrics: &ScaledAxisMetrics,
    scale: &Scale,
    base_edge_ix: usize,
    stem_edge_ix: usize,
) {
    let edges = axis.edges.as_mut_slice();
    let base_edge = &edges[base_edge_ix];
    let stem_edge = &edges[stem_edge_ix];
    let width = stem_edge.opos - base_edge.opos;
    let base_delta = base_edge.pos - base_edge.opos;
    let fitted_width = stem_width(
        axis.dim,
        metrics,
        scale,
        width,
        base_delta,
        base_edge.flags,
        stem_edge.flags,
    );
    edges[stem_edge_ix].pos = base_edge.pos + fitted_width;
}

/// Shift the serif edge by the adjustment made to base edge.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2975>
fn align_serif_edge(axis: &mut Axis, base_edge_ix: usize, serif_edge_ix: usize) {
    let edges = axis.edges.as_mut_slice();
    let base_edge = &edges[base_edge_ix];
    let serif_edge = &edges[serif_edge_ix];
    edges[serif_edge_ix].pos = base_edge.pos + (serif_edge.opos - base_edge.opos);
}

#[cfg(test)]
mod tests {
    use super::{
        super::super::{
            latin,
            metrics::{self},
            outline::Outline,
            style,
        },
        *,
    };
    use crate::MetadataProvider;
    use raw::{types::GlyphId, FontRef, TableProvider};

    #[test]
    fn edge_hinting() {
        let font = FontRef::new(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS).unwrap();
        let class = &style::STYLE_CLASSES[style::StyleClass::HEBR];
        let unscaled_metrics =
            latin::metrics::compute_unscaled_style_metrics(&font, Default::default(), class);
        let scale = metrics::Scale::new(
            16.0,
            font.head().unwrap().units_per_em() as i32,
            Default::default(),
            false,
        );
        let scaled_metrics = latin::metrics::scale_style_metrics(&unscaled_metrics, scale);
        let glyphs = font.outline_glyphs();
        let glyph = glyphs.get(GlyphId::new(9)).unwrap();
        let mut outline = Outline::default();
        outline.fill(&glyph, Default::default()).unwrap();
        let mut axes = [
            Axis::new(Axis::HORIZONTAL, outline.orientation),
            Axis::new(Axis::VERTICAL, outline.orientation),
        ];
        for (dim, axis) in axes.iter_mut().enumerate() {
            latin::segments::compute_segments(&mut outline, axis);
            latin::segments::link_segments(&outline, axis, unscaled_metrics.axes[dim].max_width());
            latin::edges::compute_edges(
                axis,
                &scaled_metrics.axes[0],
                class.script.hint_top_to_bottom,
                scaled_metrics.axes[1].scale,
            );
            if dim == Axis::VERTICAL {
                latin::edges::compute_blue_edges(
                    axis,
                    &scale,
                    &unscaled_metrics.axes[dim].blues,
                    &scaled_metrics.axes[dim].blues,
                );
            }
            hint_edges(
                axis,
                &scaled_metrics.axes[dim],
                &scale,
                class.script.hint_top_to_bottom,
            );
        }
        // Only pos and flags fields are modified by edge hinting
        let expected_h_edges = [
            (0, Edge::DONE | Edge::ROUND),
            (133, Edge::DONE),
            (187, Edge::DONE),
            (192, Edge::DONE | Edge::ROUND),
        ];
        let expected_v_edges = [
            (-256, Edge::DONE),
            (463, Edge::DONE),
            (576, Edge::DONE | Edge::ROUND | Edge::SERIF),
            (633, Edge::DONE),
        ];
        let h_edges = axes[Axis::HORIZONTAL]
            .edges
            .iter()
            .map(|edge| (edge.pos, edge.flags))
            .collect::<Vec<_>>();
        let v_edges = axes[Axis::VERTICAL]
            .edges
            .iter()
            .map(|edge| (edge.pos, edge.flags))
            .collect::<Vec<_>>();
        assert_eq!(h_edges, expected_h_edges);
        assert_eq!(v_edges, expected_v_edges);
    }
}
