//! Contains a [`Transform`] object holding values of an affine transformation matrix.
use std::ops::{Mul, MulAssign};

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::*;

use raw::types::Point;
#[cfg(test)]
use serde::{Deserialize, Serialize};

use bytemuck::{AnyBitPattern, NoUninit};

#[derive(Clone, Debug, PartialEq, AnyBitPattern, NoUninit)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
/// A transformation matrix to be applied to the drawing canvas.
///
/// Factors are specified in column-order, meaning that
/// for a vector `(x,y)` the transformed position `x'` of the vector
/// is calculated by
/// `x' = xx * x + xy * y + dx`,
/// and the transformed position y' is calculated by
/// `y' = yx * x + yy * y + dy`.
#[derive(Copy)]
#[repr(C)]
pub struct Transform {
    pub xx: f32,
    pub yx: f32,
    pub xy: f32,
    pub yy: f32,
    pub dx: f32,
    pub dy: f32,
}

impl Transform {
    /// This is equivalent to pre-multiplying this matrix by a translation
    /// matrix, but is much faster.
    pub(crate) fn pre_translate(mut self, x: f32, y: f32) -> Self {
        self.dx += x;
        self.dy += y;
        self
    }

    pub(crate) fn translate(mut self, x: f32, y: f32) -> Self {
        self.dx += (self.xx * x) + (self.xy * y);
        self.dy += (self.yx * x) + (self.yy * y);
        self
    }

    pub(crate) fn rotation_around_center(radians: f32, x: f32, y: f32) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            xx: c,
            yx: s,
            xy: -s,
            yy: c,
            dx: ((1.0 - c) * x) + (s * y),
            dy: (-s * x) + ((1.0 - c) * y)
        }
    }

    pub(crate) fn rotation(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            xx: c,
            yx: s,
            xy: -s,
            yy: c,
            dx: 0.0,
            dy: 0.0,
        }
    }

    pub(crate) fn is_translation(&self) -> bool {
        self.xx == 1.0 && self.yx == 0.0 && self.xy == 0.0 && self.yy == 1.0
    }

    pub(crate) fn transform_point(&self, Point { x, y }: Point<f32>) -> Point<f32> {
        Point::new(
            self.dx + (self.xx * x) + (self.xy * y),
            self.dy + (self.yx * x) + (self.yy * y),
        )
    }
}

impl MulAssign for Transform {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Mul for Transform {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        fn muladdmul(a: f32, b: f32, c: f32, d: f32) -> f32 {
            a * b + c * d
        }
        Self {
            xx: muladdmul(self.xx, rhs.xx, self.xy, rhs.yx),
            xy: muladdmul(self.xx, rhs.xy, self.xy, rhs.yy),
            dx: muladdmul(self.xx, rhs.dx, self.xy, rhs.dy) + self.dx,
            yx: muladdmul(self.yx, rhs.xx, self.yy, rhs.yx),
            yy: muladdmul(self.yx, rhs.xy, self.yy, rhs.yy),
            dy: muladdmul(self.yx, rhs.dx, self.yy, rhs.dy) + self.dy,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            xx: 1.0,
            yx: 0.0,
            xy: 0.0,
            yy: 1.0,
            dx: 0.0,
            dy: 0.0,
        }
    }
}
