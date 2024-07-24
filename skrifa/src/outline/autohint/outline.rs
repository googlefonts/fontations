//! Outline representation and helpers for autohinting.

use super::super::{
    unscaled::{UnscaledOutlineSink, UnscaledPoint},
    DrawError, LocationRef, OutlineGlyph,
};
use crate::collections::SmallVec;

/// Hinting directions.
///
/// The values are such that `dir1 + dir2 == 0` when the directions are
/// opposite.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L45>
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
#[repr(i8)]
pub(super) enum Direction {
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
    /// Direction of inwards vector.
    pub in_dir: Direction,
    /// Direction of outwards vector.
    pub out_dir: Direction,
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

// Matches FreeType's inline usage
//
// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.h#L332>
const MAX_INLINE_POINTS: usize = 96;
const MAX_INLINE_CONTOURS: usize = 8;

#[derive(Default)]
pub(super) struct Outline {
    pub points: SmallVec<Point, MAX_INLINE_POINTS>,
    // Range isn't Copy so can't be used in our SmallVec :(
    pub contours: SmallVec<(usize, usize), MAX_INLINE_CONTOURS>,
}

impl Outline {
    /// Fills the outline from the given glyph.
    pub fn fill(&mut self, glyph: &OutlineGlyph) -> Result<(), DrawError> {
        self.clear();
        glyph.draw_unscaled(LocationRef::default(), None, self)?;
        // Heuristic value
        let _near_limit = 20 * glyph.units_per_em() as i32 / 2048;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.contours.clear();
    }

    pub fn contours_mut(&mut self) -> impl Iterator<Item = &mut [Point]> {
        let mut points = Some(self.points.as_mut_slice());
        let mut consumed = 0;
        self.contours.iter().map(move |(_, end)| {
            let count = end - consumed;
            consumed = *end;
            let (contour_points, rest) = points.take().unwrap().split_at_mut(count);
            points = Some(rest);
            contour_points
        })
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
        let new_point_ix = self.points.len();
        if point.is_contour_start() {
            self.contours.push((new_point_ix, new_point_ix + 1));
        } else if let Some(last_contour) = self.contours.last_mut() {
            last_contour.1 += 1;
        } else {
            // If our first point is not marked as contour start, just
            // create a new contour.
            self.contours.push((new_point_ix, new_point_ix + 1));
        }
        self.points.push(new_point);
        Ok(())
    }
}

/// Iterator that begins at `start + 1` and cycles through all items
/// of the slice in forward order, ending with `start`.
pub(super) fn cycle_forward<T>(items: &[T], start: usize) -> impl Iterator<Item = (usize, &T)> {
    let len = items.len();
    let start = start + 1;
    (0..len).map(move |ix| {
        let real_ix = (ix + start) % len;
        (real_ix, &items[real_ix])
    })
}

/// Iterator that begins at `start - 1` and cycles through all items
/// of the slice in reverse order, ending with `start`.
pub(super) fn cycle_backward<T>(items: &[T], start: usize) -> impl Iterator<Item = (usize, &T)> {
    let len = items.len();
    (0..len).rev().map(move |ix| {
        let real_ix = (ix + start) % len;
        (real_ix, &items[real_ix])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_iter_forward() {
        let items = [0, 1, 2, 3, 4, 5, 6, 7];
        let from_5 = super::cycle_forward(&items, 5)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_5, &[6, 7, 0, 1, 2, 3, 4, 5]);
        let from_last = super::cycle_forward(&items, 7)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_last, &items);
    }

    #[test]
    fn cycle_iter_backward() {
        let items = [0, 1, 2, 3, 4, 5, 6, 7];
        let from_5 = super::cycle_backward(&items, 5)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_5, &[4, 3, 2, 1, 0, 7, 6, 5]);
        let from_0 = super::cycle_backward(&items, 0)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_0, &[7, 6, 5, 4, 3, 2, 1, 0]);
    }

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
}
