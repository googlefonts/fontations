//! fixed-point numerical types

use std::ops::{Add, AddAssign, Sub, SubAssign};

/// 32-bit signed fixed-point number (16.16)
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Fixed(i32);

/// 16-bit signed fixed number with the low 14 bits of fraction (2.14).
#[derive(Debug, Clone, Copy)]
pub struct F2dot14(f32);

// impl taken from pinot:
// https://github.com/dfrg/pinot/blob/master/src/types/mod.rs
impl Fixed {
    /// Minimum value.
    pub const MIN: Self = Self(0x80000000u32 as i32);

    /// Maximum value.
    pub const MAX: Self = Self(0x7FFFFFFF);

    /// Creates a 16.16 fixed point value from a 32-bit integer.
    pub const fn from_i32(x: i32) -> Self {
        Self(x << 16)
    }

    /// Creates a 16.16 fixed point value from a 32-bit floating point value.
    pub fn from_f32(x: f32) -> Self {
        Self((x * 65536. + 0.5) as i32)
    }

    /// Returns the nearest integer value.
    pub fn round(self) -> Self {
        Self(((self.0 as u32 + 0x8000) & 0xFFFF0000) as i32)
    }

    /// Returns the absolute value of the number.
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// Returns the largest integer less than or equal to the number.
    pub fn floor(self) -> Self {
        Self((self.0 as u32 & 0xFFFF0000) as i32)
    }

    /// Returns the fractional part of the number.
    pub fn fract(self) -> Self {
        Self(self.0 - self.floor().0)
    }

    /// Returns the value rounded to the nearest integer.
    pub fn to_i32(self) -> i32 {
        (self.0 + 0x8000) >> 16
    }

    /// Returns the value as a 32-bit floating point number.
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 65536.
    }
}

impl std::fmt::Display for Fixed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // is this good enough?
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for Fixed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // is this good enough?
        self.0.fmt(f)
    }
}

impl Add for Fixed {
    type Output = Self;
    #[inline(always)]
    fn add(self, other: Self) -> Self {
        // same overflow semantics as std: panic in debug, wrap in release
        Self(self.0 + other.0)
    }
}

impl AddAssign for Fixed {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl Sub for Fixed {
    type Output = Self;
    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl SubAssign for Fixed {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}
