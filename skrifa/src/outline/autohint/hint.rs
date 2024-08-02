//! Apply edge hints to an outline.
//!
//! This happens in three passes:
//! 1. Align points that are directly attached to edges.
//! 2. Interpolate non-weak points that were not touched by the previous pass.
//! 3. Interpolate remaining (weak) points.
//! 
//! The final result is a fully hinted outline.

use super::{
    axis::{Axis, Dimension},
    metrics::{fixed_div, fixed_mul, Scale},
    outline::{Outline, Point},
};
use core::cmp::Ordering;

/// Align all points of an edge to the same coordinate value.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1324>
pub(crate) fn align_edge_points(outline: &mut Outline, axis: &Axis) -> Option<()> {
    let edges = axis.edges.as_slice();
    let segments = axis.segments.as_slice();
    let points = outline.points.as_mut_slice();
    for segment in segments {
        let Some(edge) = segment.edge(edges) else {
            continue;
        };
        let mut point_ix = segment.first();
        let last_ix = segment.last();
        loop {
            let point = points.get_mut(point_ix)?;
            if axis.dim == Axis::HORIZONTAL {
                point.x = edge.pos;
                point.flags |= Point::TOUCH_X;
            } else {
                point.y = edge.pos;
                point.flags |= Point::TOUCH_Y;
            }
            if point_ix == last_ix {
                break;
            }
            point_ix = point.next();
        }
    }
    Some(())
}

/// Align the strong points; equivalent to the TrueType `IP` instruction.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1399>
pub(crate) fn align_strong_points(outline: &mut Outline, axis: &mut Axis) -> Option<()> {
    if axis.edges.is_empty() {
        return Some(());
    }
    let dim = axis.dim;
    let touch_flag = if dim == Axis::HORIZONTAL {
        Point::TOUCH_X
    } else {
        Point::TOUCH_Y
    };
    'points: for point in &mut outline.points {
        // Skip points that are already touched; do weak interpolation in the
        // next pass
        if point.flags & (touch_flag | Point::WEAK_INTERPOLATION) != 0 {
            continue;
        }
        let (u, ou) = if dim == Axis::VERTICAL {
            (point.fy, point.oy)
        } else {
            (point.fx, point.ox)
        };
        let edges = axis.edges.as_mut_slice();
        // Is the point before the first edge?
        let edge = edges.first()?;
        let delta = edge.fpos as i32 - u;
        if delta >= 0 {
            store_point(point, dim, edge.pos - (edge.opos - ou));
            continue;
        }
        // Is the point after the last edge?
        let edge = edges.last()?;
        let delta = u - edge.fpos as i32;
        if delta >= 0 {
            store_point(point, dim, edge.pos + (ou - edge.opos));
            continue;
        }
        // Find enclosing edges
        let mut min_ix = 0;
        let mut max_ix = edges.len();
        while min_ix < max_ix {
            let mid_ix = (min_ix + max_ix) >> 1;
            let edge = &edges[mid_ix];
            let fpos = edge.fpos as i32;
            match u.cmp(&fpos) {
                Ordering::Less => max_ix = mid_ix,
                Ordering::Greater => min_ix = mid_ix + 1,
                Ordering::Equal => {
                    // We are on an edge
                    store_point(point, dim, edge.pos);
                    continue 'points;
                }
            }
        }
        // Point is not on an edge
        if let Some(before_ix) = min_ix.checked_sub(1) {
            let edge_before = edges.get(before_ix)?;
            let before_pos = edge_before.pos;
            let before_fpos = edge_before.fpos as i32;
            let scale = if edge_before.scale == 0 {
                let edge_after = edges.get(min_ix)?;
                let scale = fixed_div(
                    edge_after.pos - edge_before.pos,
                    edge_after.fpos as i32 - before_fpos,
                );
                edges[before_ix].scale = scale;
                scale
            } else {
                edge_before.scale
            };
            store_point(point, dim, before_pos + fixed_mul(u - before_fpos, scale));
        }
    }
    Some(())
}

/// Align the weak points; equivalent to the TrueType `IUP` instruction.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1673>
pub(crate) fn align_weak_points(outline: &mut Outline, dim: Dimension) -> Option<()> {
    let touch_flag = if dim == Axis::HORIZONTAL {
        for point in &mut outline.points {
            point.u = point.x;
            point.v = point.ox;
        }
        Point::TOUCH_X
    } else {
        for point in &mut outline.points {
            point.u = point.y;
            point.v = point.oy;
        }
        Point::TOUCH_Y
    };
    for contour in &outline.contours {
        let points = outline.points.get_mut(contour.range())?;
        // Find first touched point
        let Some(first_touched_ix) = points
            .iter()
            .position(|point| point.flags & touch_flag != 0)
        else {
            continue;
        };
        let last_ix = points.len() - 1;
        let mut point_ix = first_touched_ix;
        let mut last_touched_ix;
        'outer: loop {
            // Skip any touched neighbors
            while point_ix < last_ix && points.get(point_ix + 1)?.flags & touch_flag != 0 {
                point_ix += 1;
            }
            last_touched_ix = point_ix;
            // Find the next touched point
            point_ix += 1;
            loop {
                if point_ix > last_ix {
                    break 'outer;
                }
                if points[point_ix].flags & touch_flag != 0 {
                    break;
                }
                point_ix += 1;
            }
            iup_interpolate(
                points,
                last_touched_ix + 1,
                point_ix - 1,
                last_touched_ix,
                point_ix,
            );
        }
        if last_touched_ix == first_touched_ix {
            // Special case: only one point was touched
            iup_shift(points, 0, last_ix, first_touched_ix);
        } else {
            // Interpolate the remainder
            if last_touched_ix < last_ix {
                iup_interpolate(
                    points,
                    last_touched_ix + 1,
                    last_ix,
                    last_touched_ix,
                    first_touched_ix,
                );
            }
            if first_touched_ix > 0 {
                iup_interpolate(
                    points,
                    0,
                    first_touched_ix - 1,
                    last_touched_ix,
                    first_touched_ix,
                );
            }
        }
    }
    // Save interpolated values
    if dim == Axis::HORIZONTAL {
        for point in &mut outline.points {
            point.x = point.u;
        }
    } else {
        for point in &mut outline.points {
            point.y = point.u;
        }
    }
    Some(())
}

#[inline(always)]
fn store_point(point: &mut Point, dim: Dimension, u: i32) {
    if dim == Axis::HORIZONTAL {
        point.x = u;
        point.flags |= Point::TOUCH_X;
    } else {
        point.y = u;
        point.flags |= Point::TOUCH_Y;
    }
}

/// Shift original coordinates of all points between `p1_ix` and `p2_ix`
/// (inclusive) to get hinted coordinates using the same difference as
/// given by the point at `ref_ix`.
///
/// The `u` and `v` members are the current and original coordinate values,
/// respectively.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1578>
fn iup_shift(points: &mut [Point], p1_ix: usize, p2_ix: usize, ref_ix: usize) -> Option<()> {
    let ref_point = points.get(ref_ix)?;
    let delta = ref_point.u - ref_point.v;
    if delta == 0 {
        return Some(());
    }
    for point in points.get_mut(p1_ix..ref_ix)? {
        point.u = point.v + delta;
    }
    for point in points.get_mut(ref_ix + 1..=p2_ix)? {
        point.u = point.v + delta;
    }
    Some(())
}

/// Interpolate the original coordinates all of points between `p1_ix` and
/// `p2_ix` (inclusive) to get hinted coordinates, using the points at
/// `ref1_ix` and `ref2_ix` as the reference points.
///
/// The `u` and `v` members are the current and original coordinate values,
/// respectively.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1605>
fn iup_interpolate(
    points: &mut [Point],
    p1_ix: usize,
    p2_ix: usize,
    ref1_ix: usize,
    ref2_ix: usize,
) -> Option<()> {
    if p1_ix > p2_ix {
        return Some(());
    }
    let mut ref_point1 = points.get(ref1_ix)?;
    let mut ref_point2 = points.get(ref2_ix)?;
    if ref_point1.v > ref_point2.v {
        core::mem::swap(&mut ref_point1, &mut ref_point2);
    }
    let (u1, v1) = (ref_point1.u, ref_point1.v);
    let (u2, v2) = (ref_point2.u, ref_point2.v);
    let d1 = u1 - v1;
    let d2 = u2 - v2;
    if u1 == u2 || v1 == v2 {
        for point in points.get_mut(p1_ix..=p2_ix)? {
            point.u = if point.v <= v1 {
                point.v + d1
            } else if point.v >= v2 {
                point.v + d2
            } else {
                u1
            };
        }
    } else {
        let scale = fixed_div(u2 - u1, v2 - v1);
        for point in points.get_mut(p1_ix..=p2_ix)? {
            point.u = if point.v <= v1 {
                point.v + d1
            } else if point.v >= v2 {
                point.v + d2
            } else {
                u1 + fixed_mul(point.v - v1, scale)
            };
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::{
        super::{latin, metrics::Scale, script},
        *,
    };
    use crate::MetadataProvider;
    use raw::{
        types::{F2Dot14, GlyphId},
        FontRef, TableProvider,
    };

    #[test]
    fn hinted_coords() {
        let font = FontRef::new(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS).unwrap();
        let outline = hint_latin_outline(
            &font,
            16.0,
            Default::default(),
            9,
            &script::SCRIPT_CLASSES[script::ScriptClass::HEBR],
        );
        #[rustfmt::skip]
        let expected_coords = [
            (133, -256),
            (133, 282),
            (133, 343),
            (146, 431),
            (158, 463),
            (158, 463),
            (57, 463),
            (30, 463),
            (0, 495),
            (0, 534),
            (0, 548),
            (2, 570),
            (11, 604),
            (17, 633),
            (50, 633),
            (50, 629),
            (50, 604),
            (77, 576),
            (101, 576),
            (163, 576),
            (180, 576),
            (192, 562),
            (192, 542),
            (192, 475),
            (190, 457),
            (187, 423),
            (187, 366),
            (187, 315),
            (187, -220),
            (178, -231),
            (159, -248),
            (146, -256),
        ];
        let coords = outline
            .points
            .iter()
            .map(|point| (point.x, point.y))
            .collect::<Vec<_>>();
        assert_eq!(coords, expected_coords);
    }

    fn hint_latin_outline(
        font: &FontRef,
        size: f32,
        coords: &[F2Dot14],
        gid: u32,
        style: &script::ScriptClass,
    ) -> Outline {
        let glyphs = font.outline_glyphs();
        let glyph = glyphs.get(GlyphId::new(gid)).unwrap();
        let mut outline = Outline::default();
        outline.fill(&glyph, coords).unwrap();
        let metrics = latin::compute_unscaled_style_metrics(font, coords, style);
        let scale = Scale::new(
            size,
            font.head().unwrap().units_per_em() as i32,
            Default::default(),
        );
        latin::hint_outline(&mut outline, &metrics, &scale);
        outline
    }
}
