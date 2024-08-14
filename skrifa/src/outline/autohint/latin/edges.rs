//! Latin edge computations.
//!
//! Edges are sets of segments that all lie within a threshold based on
//! stem widths.
//!
//! Here we compute edges from the segment list, assign properties (round,
//! serif, links) and then associate them with blue zones.

use super::super::{
    axis::{Axis, Edge, Segment},
    metrics::{fixed_div, fixed_mul, Scale, ScaledAxisMetrics, ScaledBlue, UnscaledBlue},
    outline::Direction,
    style::blue_flags,
};

/// Links segments to edges, using feature analysis for selection.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2128>
pub(crate) fn compute_edges(
    axis: &mut Axis,
    metrics: &ScaledAxisMetrics,
    top_to_bottom_hinting: bool,
    y_scale: i32,
) {
    axis.edges.clear();
    let scale = metrics.scale;
    let top_to_bottom_hinting = if axis.dim == Axis::HORIZONTAL {
        false
    } else {
        top_to_bottom_hinting
    };
    // Ignore horizontal segments less than 1 pixel in length
    let segment_length_threshold = if axis.dim == Axis::HORIZONTAL {
        fixed_div(64, y_scale)
    } else {
        0
    };
    // Also ignore segments with a width delta larger than 0.5 pixels
    let segment_width_threshold = fixed_div(32, scale);
    // Ensure that edge distance threshold is less than or equal to
    // 0.25 pixels
    let edge_distance_threshold = fixed_div(
        fixed_mul(metrics.width_metrics.edge_distance_threshold, scale).min(64 / 4),
        scale,
    );
    // Now build the sorted table of edges by looping over all segments
    // to find a matching edge, adding a new one if not found
    'segments1: for segment_ix in 0..axis.segments.len() {
        let segment = &axis.segments[segment_ix];
        // Ignore segments that are too short, too wide or direction-less
        if (segment.height as i32) < segment_length_threshold
            || (segment.delta as i32 > segment_width_threshold)
            || segment.dir == Direction::None
        {
            continue;
        }
        // Ignore serif edges that are smaller than 1.5 pixels
        if segment.serif_ix.is_some()
            && (2 * segment.height as i32) < (3 * segment_length_threshold)
        {
            continue;
        }
        // Look for a corresponding edge for this segment
        for edge_ix in 0..axis.edges.len() {
            let edge = &axis.edges[edge_ix];
            let dist = (segment.pos as i32 - edge.fpos as i32).abs();
            if dist < edge_distance_threshold && edge.dir == segment.dir {
                // We found an edge, link everything up
                axis.append_segment_to_edge(segment_ix, edge_ix);
                // Move to next segment
                continue 'segments1;
            }
        }
        // We couldn't find an edge, so add a new one for this segment
        let opos = fixed_mul(segment.pos as i32, scale);
        let edge = Edge {
            fpos: segment.pos,
            opos,
            pos: opos,
            dir: segment.dir,
            first_ix: segment_ix as u16,
            last_ix: segment_ix as u16,
            ..Default::default()
        };
        axis.insert_edge(edge, top_to_bottom_hinting);
        axis.segments[segment_ix].edge_next_ix = Some(segment_ix as u16);
    }
    // Loop again to find single point segments without a direction and
    // associate them with an existing edge if possible
    'segments2: for segment_ix in 0..axis.segments.len() {
        let segment = &axis.segments[segment_ix];
        if segment.dir != Direction::None {
            continue;
        }
        // Find a matching edge
        for edge_ix in 0..axis.edges.len() {
            let edge = &axis.edges[edge_ix];
            let dist = (segment.pos as i32 - edge.fpos as i32).abs();
            if dist < edge_distance_threshold {
                // We found an edge, link everything up
                axis.append_segment_to_edge(segment_ix, edge_ix);
                // Move to next segment
                continue 'segments2;
            }
        }
    }
    link_segments_to_edges(axis);
    compute_edge_properties(axis);
}

/// Edges get shifted and resorted as they're built so we need to assign
/// edge indices to segments in a second pass.
fn link_segments_to_edges(axis: &mut Axis) {
    let segments = axis.segments.as_mut_slice();
    for edge_ix in 0..axis.edges.len() {
        let edge = &axis.edges[edge_ix];
        let mut ix = edge.first_ix as usize;
        let last_ix = edge.last_ix as usize;
        loop {
            let segment = &mut segments[ix];
            segment.edge_ix = Some(edge_ix as u16);
            if ix == last_ix {
                break;
            }
            ix = segment
                .edge_next_ix
                .map(|ix| ix as usize)
                .unwrap_or(last_ix);
        }
    }
}

/// Compute the edge properties based on the series of segments that make
/// up the edge.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2339>
fn compute_edge_properties(axis: &mut Axis) {
    let edges = axis.edges.as_mut_slice();
    let segments = axis.segments.as_slice();
    for edge_ix in 0..edges.len() {
        let mut roundness = 0;
        let mut straightness = 0;
        let edge = edges[edge_ix];
        let mut segment_ix = edge.first_ix as usize;
        let last_segment_ix = edge.last_ix as usize;
        loop {
            let segment = &segments[segment_ix];
            let next_segment_ix = segment.edge_next_ix;
            // Check roundness
            if segment.flags & Segment::ROUND != 0 {
                roundness += 1;
            } else {
                straightness += 1;
            }
            // Check for serifs
            let is_serif = if let Some(serif_ix) = segment.serif_ix {
                let serif = &segments[serif_ix as usize];
                serif.edge_ix.is_some() && serif.edge_ix != Some(edge_ix as u16)
            } else {
                false
            };
            // Check for links
            if is_serif
                || (segment.link_ix.is_some()
                    && segments[segment.link_ix.unwrap() as usize]
                        .edge_ix
                        .is_some())
            {
                let (edge2_ix, segment2_ix) = if is_serif {
                    (edge.serif_ix, segment.serif_ix)
                } else {
                    (edge.link_ix, segment.link_ix)
                };
                let edge2_ix = if let (Some(edge2_ix), Some(segment2_ix)) = (edge2_ix, segment2_ix)
                {
                    let edge2 = &edges[edge2_ix as usize];
                    let edge_delta = (edge.fpos as i32 - edge2.fpos as i32).abs();
                    let segment2 = &segments[segment2_ix as usize];
                    let segment_delta = (segment.pos as i32 - segment2.pos as i32).abs();
                    if segment_delta < edge_delta {
                        segment2.edge_ix
                    } else {
                        Some(edge2_ix)
                    }
                } else if let Some(segment2_ix) = segment2_ix {
                    segments[segment2_ix as usize].edge_ix
                } else {
                    edge2_ix
                };
                if is_serif {
                    edges[edge_ix].serif_ix = edge2_ix;
                    edges[edge2_ix.unwrap() as usize].flags |= Edge::SERIF;
                } else {
                    edges[edge_ix].link_ix = edge2_ix;
                }
            }
            if segment_ix == last_segment_ix {
                break;
            }
            segment_ix = next_segment_ix
                .map(|ix| ix as usize)
                .unwrap_or(last_segment_ix);
        }
        let edge = &mut edges[edge_ix];
        edge.flags = Edge::NORMAL;
        if roundness > 0 && roundness >= straightness {
            edge.flags |= Edge::ROUND;
        }
        // Drop serifs for linked edges
        if edge.serif_ix.is_some() && edge.link_ix.is_some() {
            edge.serif_ix = None;
        }
    }
}

/// Compute all edges which lie within blue zones.
///
/// For Latin, this is only done for the vertical axis.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L2503>
pub(crate) fn compute_blue_edges(
    axis: &mut Axis,
    scale: &Scale,
    unscaled_blues: &[UnscaledBlue],
    blues: &[ScaledBlue],
) {
    if axis.dim != Axis::VERTICAL {
        return;
    }
    for edge in &mut axis.edges {
        let mut best_blue = None;
        let mut best_is_neutral = false;
        // Initial threshold as a fraction of em size with a max distance
        // of 0.5 pixels
        let mut best_dist = fixed_mul(scale.units_per_em / 40, scale.y_scale).min(64 / 2);
        for (unscaled_blue, blue) in unscaled_blues.iter().zip(blues) {
            // Ignore inactive blue zones
            if blue.flags & blue_flags::LATIN_ACTIVE == 0 {
                continue;
            }
            let is_top = blue.flags & (blue_flags::TOP | blue_flags::LATIN_SUB_TOP) != 0;
            let is_neutral = blue.flags & blue_flags::LATIN_NEUTRAL != 0;
            let is_major_dir = edge.dir == axis.major_dir;
            // Both directions are handled for neutral blues
            if is_top ^ is_major_dir || is_neutral {
                // Compare to reference position
                let dist = fixed_mul(
                    (edge.fpos as i32 - unscaled_blue.position).abs(),
                    scale.y_scale,
                );
                if dist < best_dist {
                    best_dist = dist;
                    best_blue = Some(blue.position);
                    best_is_neutral = is_neutral;
                }
                // Now compare to overshoot position
                if edge.flags & Edge::ROUND != 0 && dist != 0 && !is_neutral {
                    let is_under_ref = (edge.fpos as i32) < unscaled_blue.position;
                    if is_top ^ is_under_ref {
                        let dist = fixed_mul(
                            (edge.fpos as i32 - unscaled_blue.overshoot).abs(),
                            scale.y_scale,
                        );
                        if dist < best_dist {
                            best_dist = dist;
                            best_blue = Some(blue.overshoot);
                            best_is_neutral = is_neutral;
                        }
                    }
                }
            }
        }
        if let Some(best_blue) = best_blue {
            edge.blue_edge = Some(best_blue);
            if best_is_neutral {
                edge.flags |= Edge::NEUTRAL;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::super::{
            latin,
            metrics::{self, ScaledWidth},
            outline::Outline,
            style,
        },
        *,
    };
    use crate::MetadataProvider;
    use raw::{types::GlyphId, FontRef, TableProvider};

    #[test]
    fn edges() {
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
            compute_edges(
                axis,
                &scaled_metrics.axes[dim],
                class.script.hint_top_to_bottom,
                scaled_metrics.axes[1].scale,
            );
            if dim == Axis::VERTICAL {
                compute_blue_edges(
                    axis,
                    &scale,
                    &unscaled_metrics.axes[dim].blues,
                    &scaled_metrics.axes[dim].blues,
                );
            }
        }
        let expected_h_edges = [
            Edge {
                fpos: 15,
                opos: 15,
                pos: 15,
                flags: Edge::ROUND,
                dir: Direction::Up,
                blue_edge: None,
                link_ix: Some(3),
                serif_ix: None,
                scale: 0,
                first_ix: 1,
                last_ix: 1,
            },
            Edge {
                fpos: 123,
                opos: 126,
                pos: 126,
                flags: 0,
                dir: Direction::Up,
                blue_edge: None,
                link_ix: Some(2),
                serif_ix: None,
                scale: 0,
                first_ix: 0,
                last_ix: 0,
            },
            Edge {
                fpos: 186,
                opos: 190,
                pos: 190,
                flags: 0,
                dir: Direction::Down,
                blue_edge: None,
                link_ix: Some(1),
                serif_ix: None,
                scale: 0,
                first_ix: 4,
                last_ix: 4,
            },
            Edge {
                fpos: 205,
                opos: 210,
                pos: 210,
                flags: Edge::ROUND,
                dir: Direction::Down,
                blue_edge: None,
                link_ix: Some(0),
                serif_ix: None,
                scale: 0,
                first_ix: 3,
                last_ix: 3,
            },
        ];
        let expected_v_edges = [
            Edge {
                fpos: -240,
                opos: -246,
                pos: -246,
                flags: 0,
                dir: Direction::Left,
                blue_edge: Some(ScaledWidth {
                    scaled: -246,
                    fitted: -256,
                }),
                link_ix: None,
                serif_ix: Some(1),
                scale: 0,
                first_ix: 3,
                last_ix: 3,
            },
            Edge {
                fpos: 481,
                opos: 493,
                pos: 493,
                flags: 0,
                dir: Direction::Left,
                blue_edge: None,
                link_ix: Some(2),
                serif_ix: None,
                scale: 0,
                first_ix: 0,
                last_ix: 0,
            },
            Edge {
                fpos: 592,
                opos: 606,
                pos: 606,
                flags: Edge::ROUND | Edge::SERIF,
                dir: Direction::Right,
                blue_edge: Some(ScaledWidth {
                    scaled: 606,
                    fitted: 576,
                }),
                link_ix: Some(1),
                serif_ix: None,
                scale: 0,
                first_ix: 2,
                last_ix: 2,
            },
            Edge {
                fpos: 647,
                opos: 663,
                pos: 663,
                flags: 0,
                dir: Direction::Right,
                blue_edge: None,
                link_ix: None,
                serif_ix: Some(2),
                scale: 0,
                first_ix: 1,
                last_ix: 1,
            },
        ];
        assert_eq!(axes[Axis::HORIZONTAL].edges.as_slice(), &expected_h_edges);
        assert_eq!(axes[Axis::VERTICAL].edges.as_slice(), &expected_v_edges);
    }
}
