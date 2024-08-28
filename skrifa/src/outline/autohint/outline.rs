//! Outline representation and helpers for autohinting.

use super::{
    super::{
        path,
        pen::PathStyle,
        unscaled::{UnscaledOutlineSink, UnscaledPoint},
        DrawError, LocationRef, OutlineGlyph, OutlinePen,
    },
    metrics::Scale,
};
use crate::collections::SmallVec;
use core::ops::Range;
use raw::{
    tables::glyf::PointFlags,
    types::{F26Dot6, F2Dot14},
};

/// Hinting directions.
///
/// The values are such that `dir1 + dir2 == 0` when the directions are
/// opposite.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L45>
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
#[repr(i8)]
pub(crate) enum Direction {
    #[default]
    None = 4,
    Right = 1,
    Left = -1,
    Up = 2,
    Down = -2,
}

impl Direction {
    /// Computes a direction from a vector.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L751>
    pub fn new(dx: i32, dy: i32) -> Self {
        let (dir, long_arm, short_arm) = if dy >= dx {
            if dy >= -dx {
                (Direction::Up, dy, dx)
            } else {
                (Direction::Left, -dx, dy)
            }
        } else if dy >= -dx {
            (Direction::Right, dx, dy)
        } else {
            (Direction::Down, -dy, dx)
        };
        // Return no direction if arm lengths do not differ enough.
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L789>
        if long_arm <= 14 * short_arm.abs() {
            Direction::None
        } else {
            dir
        }
    }

    pub fn is_opposite(self, other: Self) -> bool {
        self as i8 + other as i8 == 0
    }

    pub fn is_same_axis(self, other: Self) -> bool {
        (self as i8).abs() == (other as i8).abs()
    }

    pub fn normalize(self) -> Self {
        // FreeType uses absolute value for this.
        match self {
            Self::Left => Self::Right,
            Self::Down => Self::Up,
            _ => self,
        }
    }
}

/// The overall orientation of an outline.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Orientation {
    Clockwise,
    CounterClockwise,
}

/// Outline point with a lot of context for hinting.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L239>
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub(super) struct Point {
    /// Describes the type and hinting state of the point.
    pub flags: u8,
    /// X coordinate in font units.
    pub fx: i32,
    /// Y coordinate in font units.
    pub fy: i32,
    /// Scaled X coordinate.
    pub ox: i32,
    /// Scaled Y coordinate.
    pub oy: i32,
    /// Hinted X coordinate.
    pub x: i32,
    /// Hinted Y coordinate.
    pub y: i32,
    /// Direction of inwards vector.
    pub in_dir: Direction,
    /// Direction of outwards vector.
    pub out_dir: Direction,
    /// Context dependent coordinate.
    pub u: i32,
    /// Context dependent coordinate.
    pub v: i32,
    /// Index of next point in contour.
    pub next_ix: u16,
    /// Index of previous point in contour.
    pub prev_ix: u16,
}

/// Point type flags.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L210>
impl Point {
    /// Quadratic control point.
    pub const QUAD: u8 = 1 << 0;
    /// Cubic control point.
    pub const CUBIC: u8 = 1 << 1;
    /// Any control point.
    pub const CONTROL: u8 = Self::QUAD | Self::CUBIC;
    /// Touched in x direction.
    pub const TOUCH_X: u8 = 1 << 2;
    /// Touched in y direction.
    pub const TOUCH_Y: u8 = 1 << 3;
    /// Candidate for weak intepolation.
    pub const WEAK_INTERPOLATION: u8 = 1 << 4;
    /// Distance to next point is very small.
    pub const NEAR: u8 = 1 << 5;
}

impl Point {
    pub fn is_on_curve(&self) -> bool {
        self.flags & Self::CONTROL == 0
    }

    /// Returns the index of the next point in the contour.
    pub fn next(&self) -> usize {
        self.next_ix as usize
    }

    /// Returns the index of the previous point in the contour.
    pub fn prev(&self) -> usize {
        self.prev_ix as usize
    }
}

// Matches FreeType's inline usage
//
// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L332>
const MAX_INLINE_POINTS: usize = 96;
const MAX_INLINE_CONTOURS: usize = 8;

#[derive(Default)]
pub(super) struct Outline {
    pub units_per_em: i32,
    pub orientation: Option<Orientation>,
    pub points: SmallVec<Point, MAX_INLINE_POINTS>,
    pub contours: SmallVec<Contour, MAX_INLINE_CONTOURS>,
}

impl Outline {
    /// Fills the outline from the given glyph.
    pub fn fill(&mut self, glyph: &OutlineGlyph, coords: &[F2Dot14]) -> Result<(), DrawError> {
        self.clear();
        glyph.draw_unscaled(LocationRef::new(coords), None, self)?;
        self.units_per_em = glyph.units_per_em() as i32;
        // Heuristic value
        let near_limit = 20 * self.units_per_em / 2048;
        self.link_points();
        self.mark_near_points(near_limit);
        self.compute_directions(near_limit);
        self.simplify_topology();
        self.check_remaining_weak_points();
        self.compute_orientation();
        Ok(())
    }

    /// Applies dimension specific scaling factors and deltas to each
    /// point in the outline.
    pub fn scale(&mut self, scale: &Scale) {
        use super::metrics::fixed_mul;
        for point in &mut self.points {
            let x = fixed_mul(point.fx, scale.x_scale) + scale.x_delta;
            let y = fixed_mul(point.fy, scale.y_scale) + scale.y_delta;
            point.ox = x;
            point.x = x;
            point.oy = y;
            point.y = y;
        }
    }

    pub fn clear(&mut self) {
        self.units_per_em = 0;
        self.points.clear();
        self.contours.clear();
    }

    pub fn to_path(
        &self,
        style: PathStyle,
        pen: &mut impl OutlinePen,
    ) -> Result<(), path::ToPathError> {
        for contour in &self.contours {
            let Some(points) = self.points.get(contour.range()) else {
                continue;
            };
            #[inline(always)]
            fn to_contour_point(point: &Point) -> path::ContourPoint<F26Dot6> {
                const FLAGS_MAP: [PointFlags; 3] = [
                    PointFlags::on_curve(),
                    PointFlags::off_curve_quad(),
                    PointFlags::off_curve_cubic(),
                ];
                path::ContourPoint {
                    x: F26Dot6::from_bits(point.x),
                    y: F26Dot6::from_bits(point.y),
                    flags: FLAGS_MAP[(point.flags & Point::CONTROL) as usize],
                }
            }
            if let Some(last_point) = points.last().map(to_contour_point) {
                path::contour_to_path(points.iter().map(to_contour_point), last_point, style, pen)?;
            }
        }
        Ok(())
    }
}

impl Outline {
    /// Sets next and previous indices for each point.
    fn link_points(&mut self) {
        let points = self.points.as_mut_slice();
        for contour in &self.contours {
            let Some(points) = points.get_mut(contour.range()) else {
                continue;
            };
            let first_ix = contour.first() as u16;
            let mut prev_ix = contour.last() as u16;
            for (ix, point) in points.iter_mut().enumerate() {
                let ix = ix as u16 + first_ix;
                point.prev_ix = prev_ix;
                prev_ix = ix;
                point.next_ix = ix + 1;
            }
            points.last_mut().unwrap().next_ix = first_ix;
        }
    }

    /// Computes the near flag for each contour.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1017>
    fn mark_near_points(&mut self, near_limit: i32) {
        let points = self.points.as_mut_slice();
        for contour in &self.contours {
            let mut prev_ix = contour.last();
            for ix in contour.range() {
                let point = points[ix];
                let prev = &mut points[prev_ix];
                // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1017>
                let out_x = point.fx - prev.fx;
                let out_y = point.fy - prev.fy;
                if out_x.abs() + out_y.abs() < near_limit {
                    prev.flags |= Point::NEAR;
                }
                prev_ix = ix;
            }
        }
    }

    /// Compute directions of in and out vectors.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1064>
    fn compute_directions(&mut self, near_limit: i32) {
        let near_limit2 = 2 * near_limit - 1;
        let points = self.points.as_mut_slice();
        for contour in &self.contours {
            // Walk backward to find the first non-near point.
            let mut first_ix = contour.first();
            let mut ix = first_ix;
            let mut prev_ix = contour.prev(first_ix);
            let mut point = points[0];
            while prev_ix != first_ix {
                let prev = points[prev_ix];
                let out_x = point.fx - prev.fx;
                let out_y = point.fy - prev.fy;
                // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1102>
                if out_x.abs() + out_y.abs() >= near_limit2 {
                    break;
                }
                point = prev;
                ix = prev_ix;
                prev_ix = contour.prev(prev_ix);
            }
            first_ix = ix;
            // Abuse u and v fields to store deltas to the next and previous
            // non-near points, respectively.
            let first = &mut points[first_ix];
            first.u = first_ix as _;
            first.v = first_ix as _;
            let mut next_ix = first_ix;
            let mut ix = first_ix;
            // Now loop over all points in the contour to compute in and
            // out directions
            let mut out_x = 0;
            let mut out_y = 0;
            loop {
                let point_ix = next_ix;
                next_ix = contour.next(point_ix);
                let point = points[point_ix];
                let next = &mut points[next_ix];
                // Accumulate the deltas until we surpass near_limit
                out_x += next.fx - point.fx;
                out_y += next.fy - point.fy;
                if out_x.abs() + out_y.abs() < near_limit {
                    next.flags |= Point::WEAK_INTERPOLATION;
                    // The original code is a do-while loop, so make
                    // sure we keep this condition before the continue
                    if next_ix == first_ix {
                        break;
                    }
                    continue;
                }
                let out_dir = Direction::new(out_x, out_y);
                next.in_dir = out_dir;
                next.v = ix as _;
                let cur = &mut points[ix];
                cur.u = next_ix as _;
                cur.out_dir = out_dir;
                // Adjust directions for all intermediate points
                let mut inter_ix = contour.next(ix);
                while inter_ix != next_ix {
                    let point = &mut points[inter_ix];
                    point.in_dir = out_dir;
                    point.out_dir = out_dir;
                    inter_ix = contour.next(inter_ix);
                }
                ix = next_ix;
                points[ix].u = first_ix as _;
                points[first_ix].v = ix as _;
                out_x = 0;
                out_y = 0;
                if next_ix == first_ix {
                    break;
                }
            }
        }
    }

    /// Simplify so that we can identify local extrema more reliably.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1181>
    fn simplify_topology(&mut self) {
        let points = self.points.as_mut_slice();
        for i in 0..points.len() {
            let point = points[i];
            if point.in_dir == Direction::None && point.out_dir == Direction::None {
                let u_index = point.u as usize;
                let v_index = point.v as usize;
                let next_u = points[u_index];
                let prev_v = points[v_index];
                let in_x = point.fx - prev_v.fx;
                let in_y = point.fy - prev_v.fy;
                let out_x = next_u.fx - point.fx;
                let out_y = next_u.fy - point.fy;
                if (in_x ^ out_x) >= 0 || (in_y ^ out_y) >= 0 {
                    // Both vectors point into the same quadrant
                    points[i].flags |= Point::WEAK_INTERPOLATION;
                    points[v_index].u = u_index as _;
                    points[u_index].v = v_index as _;
                }
            }
        }
    }

    /// Check for remaining weak points.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L1226>
    fn check_remaining_weak_points(&mut self) {
        let points = self.points.as_mut_slice();
        for i in 0..points.len() {
            let point = points[i];
            let mut make_weak = false;
            if point.flags & Point::WEAK_INTERPOLATION != 0 {
                // Already weak
                continue;
            }
            if point.flags & Point::CONTROL != 0 {
                // Control points are always weak
                make_weak = true;
            } else if point.out_dir == point.in_dir {
                if point.out_dir != Direction::None {
                    // Point lies on a vertical or horizontal segment but
                    // not at start or end
                    make_weak = true;
                } else {
                    let u_index = point.u as usize;
                    let v_index = point.v as usize;
                    let next_u = points[u_index];
                    let prev_v = points[v_index];
                    if is_corner_flat(
                        point.fx - prev_v.fx,
                        point.fy - prev_v.fy,
                        next_u.fx - point.fx,
                        next_u.fy - point.fy,
                    ) {
                        // One of the vectors is more dominant
                        make_weak = true;
                        points[v_index].u = u_index as _;
                        points[u_index].v = v_index as _;
                    }
                }
            } else if point.in_dir.is_opposite(point.out_dir) {
                // Point forms a "spike"
                make_weak = true;
            }
            if make_weak {
                points[i].flags |= Point::WEAK_INTERPOLATION;
            }
        }
    }

    /// Computes the overall winding order of the outline.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/base/ftoutln.c#L1049>
    fn compute_orientation(&mut self) {
        self.orientation = None;
        let points = self.points.as_slice();
        if points.is_empty() {
            return;
        }
        // Compute cbox. Is this necessary?
        let (x_min, x_max, y_min, y_max) = {
            let first = &points[0];
            let mut x_min = first.fx;
            let mut x_max = x_min;
            let mut y_min = first.fy;
            let mut y_max = y_min;
            for point in &points[1..] {
                x_min = point.fx.min(x_min);
                x_max = point.fx.max(x_max);
                y_min = point.fy.min(y_min);
                y_max = point.fy.max(y_max);
            }
            (x_min, x_max, y_min, y_max)
        };
        // Reject empty outlines
        if x_min == x_max || y_min == y_max {
            return;
        }
        // Reject large outlines
        const MAX_COORD: i32 = 0x1000000;
        if x_min < -MAX_COORD || x_max > MAX_COORD || y_min < -MAX_COORD || y_max > MAX_COORD {
            return;
        }
        fn msb(x: u32) -> i32 {
            (31 - x.leading_zeros()) as i32
        }
        let x_shift = msb((x_max.abs() | x_min.abs()) as u32) - 14;
        let x_shift = x_shift.max(0);
        let y_shift = msb((y_max - y_min) as u32) - 14;
        let y_shift = y_shift.max(0);
        let shifted_point = |ix: usize| {
            let point = &points[ix];
            (point.fx >> x_shift, point.fy >> y_shift)
        };
        let mut area = 0;
        for contour in &self.contours {
            let last_ix = contour.last();
            let first_ix = contour.first();
            let (mut prev_x, mut prev_y) = shifted_point(last_ix);
            for i in first_ix..=last_ix {
                let (x, y) = shifted_point(i);
                area += (y - prev_y) * (x + prev_x);
                (prev_x, prev_y) = (x, y);
            }
        }
        use core::cmp::Ordering;
        self.orientation = match area.cmp(&0) {
            Ordering::Less => Some(Orientation::CounterClockwise),
            Ordering::Greater => Some(Orientation::Clockwise),
            Ordering::Equal => None,
        };
    }
}

/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/base/ftcalc.c#L1026>
fn is_corner_flat(in_x: i32, in_y: i32, out_x: i32, out_y: i32) -> bool {
    let ax = in_x + out_x;
    let ay = in_y + out_y;
    fn hypot(x: i32, y: i32) -> i32 {
        let x = x.abs();
        let y = y.abs();
        if x > y {
            x + ((3 * y) >> 3)
        } else {
            y + ((3 * x) >> 3)
        }
    }
    let d_in = hypot(in_x, in_y);
    let d_out = hypot(out_x, out_y);
    let d_hypot = hypot(ax, ay);
    (d_in + d_out - d_hypot) < (d_hypot >> 4)
}

#[derive(Copy, Clone, Default, Debug)]
pub(super) struct Contour {
    first_ix: u16,
    last_ix: u16,
}

impl Contour {
    pub fn first(self) -> usize {
        self.first_ix as usize
    }

    pub fn last(self) -> usize {
        self.last_ix as usize
    }

    pub fn next(self, index: usize) -> usize {
        if index >= self.last_ix as usize {
            self.first_ix as usize
        } else {
            index + 1
        }
    }

    pub fn prev(self, index: usize) -> usize {
        if index <= self.first_ix as usize {
            self.last_ix as usize
        } else {
            index - 1
        }
    }

    pub fn range(self) -> Range<usize> {
        self.first()..self.last() + 1
    }
}

impl UnscaledOutlineSink for Outline {
    fn try_reserve(&mut self, additional: usize) -> Result<(), DrawError> {
        if self.points.try_reserve(additional) {
            Ok(())
        } else {
            Err(DrawError::InsufficientMemory)
        }
    }

    fn push(&mut self, point: UnscaledPoint) -> Result<(), DrawError> {
        let flags = if point.is_off_curve_quad() {
            Point::QUAD
        } else if point.is_off_curve_cubic() {
            Point::CUBIC
        } else {
            0
        };
        let new_point = Point {
            flags,
            fx: point.x as i32,
            fy: point.y as i32,
            ..Default::default()
        };
        let new_point_ix: u16 = self
            .points
            .len()
            .try_into()
            .map_err(|_| DrawError::InsufficientMemory)?;
        if point.is_contour_start() {
            self.contours.push(Contour {
                first_ix: new_point_ix,
                last_ix: new_point_ix,
            });
        } else if let Some(last_contour) = self.contours.last_mut() {
            last_contour.last_ix += 1;
        } else {
            // If our first point is not marked as contour start, just
            // create a new contour.
            self.contours.push(Contour {
                first_ix: new_point_ix,
                last_ix: new_point_ix,
            });
        }
        self.points.push(new_point);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{pen::SvgPen, DrawSettings};
    use super::*;
    use crate::{prelude::Size, MetadataProvider};
    use raw::{types::GlyphId, FontRef, TableProvider};

    #[test]
    fn direction_from_vectors() {
        assert_eq!(Direction::new(-100, 0), Direction::Left);
        assert_eq!(Direction::new(100, 0), Direction::Right);
        assert_eq!(Direction::new(0, -100), Direction::Down);
        assert_eq!(Direction::new(0, 100), Direction::Up);
        assert_eq!(Direction::new(7, 100), Direction::Up);
        // This triggers the too close heuristic
        assert_eq!(Direction::new(8, 100), Direction::None);
    }

    #[test]
    fn direction_axes() {
        use Direction::*;
        let hori = [Left, Right];
        let vert = [Up, Down];
        for h in hori {
            for h2 in hori {
                assert!(h.is_same_axis(h2));
                if h != h2 {
                    assert!(h.is_opposite(h2));
                } else {
                    assert!(!h.is_opposite(h2));
                }
            }
            for v in vert {
                assert!(!h.is_same_axis(v));
                assert!(!h.is_opposite(v));
            }
        }
        for v in vert {
            for v2 in vert {
                assert!(v.is_same_axis(v2));
                if v != v2 {
                    assert!(v.is_opposite(v2));
                } else {
                    assert!(!v.is_opposite(v2));
                }
            }
        }
    }

    #[test]
    fn fill_outline() {
        let outline = make_outline(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS, 8);
        use Direction::*;
        let expected = &[
            // (x, y, in_dir, out_dir, flags)
            (107, 0, Left, Left, 16),
            (85, 0, Left, None, 17),
            (55, 26, None, Up, 17),
            (55, 71, Up, Up, 16),
            (55, 332, Up, Up, 16),
            (55, 360, Up, None, 17),
            (67, 411, None, None, 17),
            (93, 459, None, None, 17),
            (112, 481, None, Up, 0),
            (112, 504, Up, Right, 0),
            (168, 504, Right, Down, 0),
            (168, 483, Down, None, 0),
            (153, 473, None, None, 17),
            (126, 428, None, None, 17),
            (109, 366, None, Down, 17),
            (109, 332, Down, Down, 16),
            (109, 109, Down, Right, 0),
            (407, 109, Right, Right, 16),
            (427, 109, Right, None, 17),
            (446, 136, None, None, 17),
            (453, 169, None, Up, 17),
            (453, 178, Up, Up, 16),
            (453, 374, Up, Up, 16),
            (453, 432, Up, None, 17),
            (400, 483, None, Left, 17),
            (362, 483, Left, Left, 16),
            (109, 483, Left, Left, 16),
            (86, 483, Left, None, 17),
            (62, 517, None, Up, 17),
            (62, 555, Up, Up, 16),
            (62, 566, Up, None, 17),
            (64, 587, None, None, 17),
            (71, 619, None, None, 17),
            (76, 647, None, Right, 0),
            (103, 647, Right, Down, 32),
            (103, 644, Down, Down, 16),
            (103, 619, Down, None, 17),
            (131, 592, None, Right, 17),
            (155, 592, Right, Right, 16),
            (386, 592, Right, Right, 16),
            (437, 592, Right, None, 17),
            (489, 552, None, None, 17),
            (507, 485, None, Down, 17),
            (507, 443, Down, Down, 16),
            (507, 75, Down, Down, 16),
            (507, 40, Down, None, 17),
            (470, 0, None, Left, 17),
            (436, 0, Left, Left, 16),
        ];
        let points = outline
            .points
            .iter()
            .map(|point| {
                (
                    point.fx,
                    point.fy,
                    point.in_dir,
                    point.out_dir,
                    point.flags as i32,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(&points, expected);
    }

    #[test]
    fn orientation() {
        let tt_outline = make_outline(font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS, 8);
        // TrueType outlines are counter clockwise
        assert_eq!(tt_outline.orientation, Some(Orientation::CounterClockwise));
        let ps_outline = make_outline(font_test_data::CANTARELL_VF_TRIMMED, 4);
        // PostScript outlines are clockwise
        assert_eq!(ps_outline.orientation, Some(Orientation::Clockwise));
    }

    fn make_outline(font_data: &[u8], glyph_id: u32) -> Outline {
        let font = FontRef::new(font_data).unwrap();
        let glyphs = font.outline_glyphs();
        let glyph = glyphs.get(GlyphId::from(glyph_id)).unwrap();
        let mut outline = Outline::default();
        outline.fill(&glyph, Default::default()).unwrap();
        outline
    }

    /// Ensure that autohint outline to path conversion produces the same
    /// output as our other scalers for all formats and path styles including
    /// edge cases like mostly off-curve glyphs
    #[test]
    fn outline_to_path_conversion() {
        compare_outlines("MOSTLY_OFF_CURVE", font_test_data::MOSTLY_OFF_CURVE);
        compare_outlines("STARTING_OFF_CURVE", font_test_data::STARTING_OFF_CURVE);
        compare_outlines("CUBIC_GLYF", font_test_data::CUBIC_GLYF);
        compare_outlines("VAZIRMATN_VAR", font_test_data::VAZIRMATN_VAR);
        compare_outlines("CANTARELL_VF_TRIMMED", font_test_data::CANTARELL_VF_TRIMMED);
        compare_outlines(
            "NOTO_SERIF_DISPLAY_TRIMMED",
            font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
        );
    }

    fn compare_outlines(font_name: &str, font_data: &[u8]) {
        let font = FontRef::new(font_data).unwrap();
        let glyph_count = font.maxp().unwrap().num_glyphs();
        let glyphs = font.outline_glyphs();
        // Check both path styles
        for path_style in [PathStyle::FreeType, PathStyle::HarfBuzz] {
            // And all glyphs
            for gid in 0..glyph_count {
                let glyph = glyphs.get(GlyphId::from(gid)).unwrap();
                // Unscaled, unhinted code path
                let mut base_svg = SvgPen::default();
                let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default())
                    .with_path_style(path_style);
                glyph.draw(settings, &mut base_svg);
                let base_svg = base_svg.to_string();
                // Autohinter outline code path
                let mut outline = Outline::default();
                outline.fill(&glyph, Default::default()).unwrap();
                // The to_path method uses the (x, y) coords which aren't filled
                // until we scale (and we aren't doing that here) so update
                // them with 26.6 values manually
                for point in &mut outline.points {
                    point.x = point.fx << 6;
                    point.y = point.fy << 6;
                }
                let mut autohint_svg = SvgPen::default();
                outline.to_path(path_style, &mut autohint_svg);
                let autohint_svg = autohint_svg.to_string();
                assert_eq!(
                    base_svg, autohint_svg,
                    "autohint outline to path comparison failed for glyph {gid} of font {font_name} with style {path_style:?}"
                );
            }
        }
    }
}
