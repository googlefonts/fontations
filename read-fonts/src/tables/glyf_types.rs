use bytemuck::AnyBitPattern;
use core::ops::{Add, AddAssign, Div, Mul, MulAssign, Sub};
use types::Fixed;

/// Marker bits for point flags that are set during variation delta
/// processing and hinting.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct PointMarker(pub(super) u8);

impl PointMarker {
    /// Marker for points that have an explicit delta in a glyph variation
    /// tuple.
    pub const HAS_DELTA: Self = Self(0x4);

    /// Marker that signifies that the x coordinate of a point has been touched
    /// by an IUP hinting instruction.
    pub const TOUCHED_X: Self = Self(0x10);

    /// Marker that signifies that the y coordinate of a point has been touched
    /// by an IUP hinting instruction.
    pub const TOUCHED_Y: Self = Self(0x20);

    /// Marker that signifies that the both coordinates of a point has been touched
    /// by an IUP hinting instruction.
    pub const TOUCHED: Self = Self(Self::TOUCHED_X.0 | Self::TOUCHED_Y.0);

    /// Marks this point as a candidate for weak interpolation.
    ///
    /// Used by the automatic hinter.
    pub const WEAK_INTERPOLATION: Self = Self(0x2);

    /// Marker for points where the distance to next point is very small.
    ///
    /// Used by the automatic hinter.
    pub const NEAR: PointMarker = Self(0x8);
}

impl core::ops::BitOr for PointMarker {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// Flags describing the properties of a point.
///
/// Some properties, such as on- and off-curve flags are intrinsic to the point
/// itself. Others, designated as markers are set and cleared while an outline
/// is being transformed during variation application and hinting.
#[derive(
    Copy, Clone, PartialEq, Eq, Default, Debug, bytemuck::AnyBitPattern, bytemuck::NoUninit,
)]
#[repr(transparent)]
pub struct PointFlags(pub(super) u8);

impl PointFlags {
    // Note: OFF_CURVE_QUAD is signified by the absence of both ON_CURVE
    // and OFF_CURVE_CUBIC bits, per FreeType and TrueType convention.
    pub(super) const ON_CURVE: u8 = 0x01;
    pub(super) const OFF_CURVE_CUBIC: u8 = 0x80;
    pub(super) const CURVE_MASK: u8 = Self::ON_CURVE | Self::OFF_CURVE_CUBIC;

    /// Creates a new on curve point flag.
    pub const fn on_curve() -> Self {
        Self(Self::ON_CURVE)
    }

    /// Creates a new off curve quadratic point flag.
    pub const fn off_curve_quad() -> Self {
        Self(0)
    }

    /// Creates a new off curve cubic point flag.
    pub const fn off_curve_cubic() -> Self {
        Self(Self::OFF_CURVE_CUBIC)
    }

    /// Creates a point flag from the given bits. These are truncated
    /// to ignore markers.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & Self::CURVE_MASK)
    }

    /// Returns true if this is an on curve point.
    #[inline]
    pub const fn is_on_curve(self) -> bool {
        self.0 & Self::ON_CURVE != 0
    }

    /// Returns true if this is an off curve quadratic point.
    #[inline]
    pub const fn is_off_curve_quad(self) -> bool {
        self.0 & Self::CURVE_MASK == 0
    }

    /// Returns true if this is an off curve cubic point.
    #[inline]
    pub const fn is_off_curve_cubic(self) -> bool {
        self.0 & Self::OFF_CURVE_CUBIC != 0
    }

    pub const fn is_off_curve(self) -> bool {
        self.is_off_curve_quad() || self.is_off_curve_cubic()
    }

    /// Flips the state of the on curve flag.
    ///
    /// This is used for the TrueType `FLIPPT` instruction.
    pub fn flip_on_curve(&mut self) {
        self.0 ^= 1;
    }

    /// Enables the on curve flag.
    ///
    /// This is used for the TrueType `FLIPRGON` instruction.
    pub fn set_on_curve(&mut self) {
        self.0 |= Self::ON_CURVE;
    }

    /// Disables the on curve flag.
    ///
    /// This is used for the TrueType `FLIPRGOFF` instruction.
    pub fn clear_on_curve(&mut self) {
        self.0 &= !Self::ON_CURVE;
    }

    /// Returns true if the given marker is set for this point.
    pub fn has_marker(self, marker: PointMarker) -> bool {
        self.0 & marker.0 != 0
    }

    /// Applies the given marker to this point.
    pub fn set_marker(&mut self, marker: PointMarker) {
        self.0 |= marker.0;
    }

    /// Clears the given marker for this point.
    pub fn clear_marker(&mut self, marker: PointMarker) {
        self.0 &= !marker.0
    }

    /// Returns a copy with all markers cleared.
    pub const fn without_markers(self) -> Self {
        Self(self.0 & Self::CURVE_MASK)
    }

    /// Returns the underlying bits.
    pub const fn to_bits(self) -> u8 {
        self.0
    }
}

/// Trait for types that are usable for TrueType point coordinates.
pub trait PointCoord:
    Copy
    + Default
    // You could bytemuck with me
    + AnyBitPattern
    // You could compare me
    + PartialEq
    + PartialOrd
    // You could do math with me
    + Add<Output = Self>
    + AddAssign
    + Sub<Output = Self>
    + Div<Output = Self>
    + Mul<Output = Self>
    + MulAssign {
    fn from_fixed(x: Fixed) -> Self;
    fn from_i32(x: i32) -> Self;
    fn to_f32(self) -> f32;
    fn midpoint(self, other: Self) -> Self;
}
