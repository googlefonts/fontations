//! Glyph zones.

use read_fonts::{
    tables::glyf::{PointFlags, PointMarker},
    types::Point,
};

use super::{
    super::{error::HintErrorKind, graphics_state::CoordAxis, math},
    GraphicsState,
};

use HintErrorKind::{InvalidPointIndex, InvalidPointRange};

/// Reference to either the twilight or glyph zone.
///
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructing_glyphs#zones>
#[derive(Copy, Clone, PartialEq, Default, Debug)]
#[repr(u8)]
pub enum ZonePointer {
    Twilight = 0,
    #[default]
    Glyph = 1,
}

impl ZonePointer {
    pub fn is_twilight(self) -> bool {
        self == Self::Twilight
    }

    pub fn is_glyph(self) -> bool {
        self == Self::Glyph
    }
}

impl TryFrom<i32> for ZonePointer {
    type Error = HintErrorKind;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Twilight),
            1 => Ok(Self::Glyph),
            _ => Err(HintErrorKind::InvalidZoneIndex(value)),
        }
    }
}

/// Glyph zone for TrueType hinting.
///
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructing_glyphs#zones>
#[derive(Default, Debug)]
pub struct Zone<'a> {
    /// Outline points prior to applying scale.
    pub unscaled: &'a mut [Point<i32>],
    /// Copy of the outline points after applying scale.
    pub original: &'a mut [Point<i32>],
    /// Scaled outline points.
    pub points: &'a mut [Point<i32>],
    pub flags: &'a mut [PointFlags],
    pub contours: &'a [u16],
}

impl<'a> Zone<'a> {
    /// Creates a new hinting zone.
    pub fn new(
        unscaled: &'a mut [Point<i32>],
        original: &'a mut [Point<i32>],
        points: &'a mut [Point<i32>],
        flags: &'a mut [PointFlags],
        contours: &'a [u16],
    ) -> Self {
        Self {
            unscaled,
            original,
            points,
            flags,
            contours,
        }
    }

    pub fn clear(&mut self) {
        for p in self
            .unscaled
            .iter_mut()
            .chain(self.original.iter_mut())
            .chain(self.points.iter_mut())
        {
            *p = Point::default();
        }
    }

    pub fn point(&self, index: usize) -> Result<Point<i32>, HintErrorKind> {
        self.points
            .get(index)
            .copied()
            .ok_or(InvalidPointIndex(index))
    }

    pub fn point_mut(&mut self, index: usize) -> Result<&mut Point<i32>, HintErrorKind> {
        self.points.get_mut(index).ok_or(InvalidPointIndex(index))
    }

    pub fn original(&self, index: usize) -> Result<Point<i32>, HintErrorKind> {
        self.original
            .get(index)
            .copied()
            .ok_or(InvalidPointIndex(index))
    }

    pub fn original_mut(&mut self, index: usize) -> Result<&mut Point<i32>, HintErrorKind> {
        self.original.get_mut(index).ok_or(InvalidPointIndex(index))
    }

    pub fn unscaled(&self, index: usize) -> Result<Point<i32>, HintErrorKind> {
        self.unscaled
            .get(index)
            .copied()
            .ok_or(InvalidPointIndex(index))
    }

    pub fn contour(&self, index: usize) -> Result<u16, HintErrorKind> {
        self.contours
            .get(index)
            .copied()
            .ok_or(HintErrorKind::InvalidContourIndex(index))
    }

    pub fn touch(&mut self, index: usize, axis: CoordAxis) -> Result<(), HintErrorKind> {
        let flag = self.flags.get_mut(index).ok_or(InvalidPointIndex(index))?;
        flag.set_marker(axis.touched_marker());
        Ok(())
    }

    pub fn untouch(&mut self, index: usize, axis: CoordAxis) -> Result<(), HintErrorKind> {
        let flag = self.flags.get_mut(index).ok_or(InvalidPointIndex(index))?;
        flag.clear_marker(axis.touched_marker());
        Ok(())
    }

    pub fn is_touched(&self, index: usize, axis: CoordAxis) -> Result<bool, HintErrorKind> {
        let flag = self.flags.get(index).ok_or(InvalidPointIndex(index))?;
        Ok(flag.has_marker(axis.touched_marker()))
    }

    pub fn flip_on_curve(&mut self, index: usize) -> Result<(), HintErrorKind> {
        let flag = self.flags.get_mut(index).ok_or(InvalidPointIndex(index))?;
        flag.flip_on_curve();
        Ok(())
    }

    pub fn set_on_curve(
        &mut self,
        start: usize,
        end: usize,
        on: bool,
    ) -> Result<(), HintErrorKind> {
        let flags = self
            .flags
            .get_mut(start..end)
            .ok_or(InvalidPointRange(start, end))?;
        if on {
            for flag in flags {
                flag.set_on_curve();
            }
        } else {
            for flag in flags {
                flag.clear_on_curve();
            }
        }
        Ok(())
    }

    /// Interpolate untouched points.
    ///
    /// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6391>
    pub fn iup(&mut self, axis: CoordAxis) -> Result<(), HintErrorKind> {
        let mut point = 0;
        for i in 0..self.contours.len() {
            let mut end_point = self.contour(i)? as usize;
            let first_point = point;
            if end_point >= self.points.len() {
                end_point = self.points.len() - 1;
            }
            while point <= end_point && !self.is_touched(point, axis)? {
                point += 1;
            }
            if point <= end_point {
                let first_touched = point;
                let mut cur_touched = point;
                point += 1;
                while point <= end_point {
                    if self.is_touched(point, axis)? {
                        self.iup_interpolate(axis, cur_touched + 1, point - 1, cur_touched, point)?;
                        cur_touched = point;
                    }
                    point += 1;
                }
                if cur_touched == first_touched {
                    self.iup_shift(axis, first_point, end_point, cur_touched)?;
                } else {
                    self.iup_interpolate(
                        axis,
                        cur_touched + 1,
                        end_point,
                        cur_touched,
                        first_touched,
                    )?;
                    if first_touched > 0 {
                        self.iup_interpolate(
                            axis,
                            first_point,
                            first_touched - 1,
                            cur_touched,
                            first_touched,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Shift the range of points p1..=p2 based on the delta given by the
    /// reference point p.
    ///
    /// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6262>
    fn iup_shift(
        &mut self,
        axis: CoordAxis,
        p1: usize,
        p2: usize,
        p: usize,
    ) -> Result<(), HintErrorKind> {
        if p1 > p2 || p1 > p || p > p2 {
            return Ok(());
        }
        macro_rules! shift_coord {
            ($coord:ident) => {
                let delta = self.point(p)?.$coord - self.original(p)?.$coord;
                if delta != 0 {
                    let (first, second) = self
                        .points
                        .get_mut(p1..=p2)
                        .ok_or(InvalidPointRange(p1, p2 + 1))?
                        .split_at_mut(p - p1);
                    for point in first
                        .iter_mut()
                        .chain(second.get_mut(1..).ok_or(InvalidPointIndex(p - p1))?)
                    {
                        point.$coord += delta;
                    }
                }
            };
        }
        if axis == CoordAxis::X {
            shift_coord!(x);
        } else {
            shift_coord!(y);
        }
        Ok(())
    }

    /// Interpolate the range of points p1..=p2 based on the deltas
    /// given by the two reference points.
    ///
    /// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6284>
    fn iup_interpolate(
        &mut self,
        axis: CoordAxis,
        p1: usize,
        p2: usize,
        mut ref1: usize,
        mut ref2: usize,
    ) -> Result<(), HintErrorKind> {
        if p1 > p2 {
            return Ok(());
        }
        let max_points = self.points.len();
        if ref1 >= max_points || ref2 >= max_points {
            return Ok(());
        }
        macro_rules! interpolate_coord {
            ($coord:ident) => {
                let mut orus1 = self.unscaled(ref1)?.$coord;
                let mut orus2 = self.unscaled(ref2)?.$coord;
                if orus1 > orus2 {
                    use core::mem::swap;
                    swap(&mut orus1, &mut orus2);
                    swap(&mut ref1, &mut ref2);
                }
                let org1 = self.original(ref1)?.$coord;
                let org2 = self.original(ref2)?.$coord;
                let cur1 = self.point(ref1)?.$coord;
                let cur2 = self.point(ref2)?.$coord;
                let delta1 = cur1 - org1;
                let delta2 = cur2 - org2;
                let iter = self
                    .original
                    .get(p1..=p2)
                    .ok_or(InvalidPointRange(p1, p2 + 1))?
                    .iter()
                    .zip(
                        self.unscaled
                            .get(p1..=p2)
                            .ok_or(InvalidPointRange(p1, p2 + 1))?,
                    )
                    .zip(
                        self.points
                            .get_mut(p1..=p2)
                            .ok_or(InvalidPointRange(p1, p2 + 1))?,
                    );
                if cur1 == cur2 || orus1 == orus2 {
                    for ((orig, _unscaled), point) in iter {
                        let a = orig.$coord;
                        point.$coord = if a <= org1 {
                            a + delta1
                        } else if a >= org2 {
                            a + delta2
                        } else {
                            cur1
                        };
                    }
                } else {
                    let scale = math::div(cur2 - cur1, orus2 - orus1);
                    for ((orig, unscaled), point) in iter {
                        let a = orig.$coord;
                        point.$coord = if a <= org1 {
                            a + delta1
                        } else if a >= org2 {
                            a + delta2
                        } else {
                            cur1 + math::mul(unscaled.$coord - orus1, scale)
                        };
                    }
                }
            };
        }
        if axis == CoordAxis::X {
            interpolate_coord!(x);
        } else {
            interpolate_coord!(y);
        }
        Ok(())
    }
}

impl<'a> GraphicsState<'a> {
    pub fn reset_zone_pointers(&mut self) {
        self.zp0 = ZonePointer::default();
        self.zp1 = ZonePointer::default();
        self.zp2 = ZonePointer::default();
    }

    #[inline(always)]
    pub fn zone(&self, pointer: ZonePointer) -> &Zone<'a> {
        &self.zones[pointer as usize]
    }

    #[inline(always)]
    pub fn zone_mut(&mut self, pointer: ZonePointer) -> &mut Zone<'a> {
        &mut self.zones[pointer as usize]
    }

    #[inline(always)]
    pub fn zp0(&self) -> &Zone<'a> {
        self.zone(self.zp0)
    }

    #[inline(always)]
    pub fn zp0_mut(&mut self) -> &mut Zone<'a> {
        self.zone_mut(self.zp0)
    }

    #[inline(always)]
    pub fn zp1(&self) -> &Zone {
        self.zone(self.zp1)
    }

    #[inline(always)]
    pub fn zp1_mut(&mut self) -> &mut Zone<'a> {
        self.zone_mut(self.zp1)
    }

    #[inline(always)]
    pub fn zp2(&self) -> &Zone {
        self.zone(self.zp2)
    }

    #[inline(always)]
    pub fn zp2_mut(&mut self) -> &mut Zone<'a> {
        self.zone_mut(self.zp2)
    }
}

impl GraphicsState<'_> {
    /// Moves the requested original point by the given distance.
    // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L1743>
    pub(crate) fn move_original(
        &mut self,
        zone: ZonePointer,
        point_ix: usize,
        distance: i32,
    ) -> Result<(), HintErrorKind> {
        let fv = self.freedom_vector;
        let fdotp = self.fdotp;
        let axis = self.freedom_axis;
        let point = self.zone_mut(zone).original_mut(point_ix)?;
        match axis {
            CoordAxis::X => point.x += distance,
            CoordAxis::Y => point.y += distance,
            CoordAxis::Both => {
                if fv.x != 0 {
                    point.x += math::mul_div(distance, fv.x, fdotp);
                }
                if fv.y != 0 {
                    point.y += math::mul_div(distance, fv.y, fdotp);
                }
            }
        }
        Ok(())
    }

    /// Moves the requested scaled point by the given distance.
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L1771>
    pub(crate) fn move_point(
        &mut self,
        zone: ZonePointer,
        point_ix: usize,
        distance: i32,
    ) -> Result<(), HintErrorKind> {
        // Note: we never adjust x in backward compatibility mode and we never
        // adjust y in backward compability mode after IUP has been done in
        // both directions.
        //
        // The primary motivation is to avoid horizontal adjustments in cases
        // where subpixel rendering provides better fidelity.
        //
        // For more detail, see <https://learn.microsoft.com/en-us/typography/cleartype/truetypecleartype>
        let back_compat = self.backward_compatibility;
        let back_compat_and_did_iup = back_compat && self.did_iup_x && self.did_iup_y;
        let zone = &mut self.zones[zone as usize];
        let point = zone.point_mut(point_ix)?;
        match self.freedom_axis {
            CoordAxis::X => {
                if !back_compat {
                    point.x += distance;
                }
                zone.touch(point_ix, CoordAxis::X)?;
            }
            CoordAxis::Y => {
                if !back_compat_and_did_iup {
                    point.y += distance;
                }
                zone.touch(point_ix, CoordAxis::Y)?;
            }
            CoordAxis::Both => {
                // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L1669>
                let fv = self.freedom_vector;
                if fv.x != 0 {
                    if !back_compat {
                        point.x += math::mul_div(distance, fv.x, self.fdotp);
                    }
                    zone.touch(point_ix, CoordAxis::X)?;
                }
                if fv.y != 0 {
                    if !back_compat_and_did_iup {
                        zone.point_mut(point_ix)?.y += math::mul_div(distance, fv.y, self.fdotp);
                    }
                    zone.touch(point_ix, CoordAxis::Y)?;
                }
            }
        }
        Ok(())
    }

    /// Moves the requested scaled point in the zone referenced by zp2 by the
    /// given delta.
    ///
    /// This is a helper function for SHP, SHC, SHZ, and SHPIX instructions.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5170>
    pub(crate) fn move_zp2_point(
        &mut self,
        point_ix: usize,
        dx: i32,
        dy: i32,
        do_touch: bool,
    ) -> Result<(), HintErrorKind> {
        // See notes above in move_point() about how this is used.
        let back_compat = self.backward_compatibility;
        let back_compat_and_did_iup = back_compat && self.did_iup_x && self.did_iup_y;
        let fv = self.freedom_vector;
        let zone = self.zp2_mut();
        if fv.x != 0 {
            if !back_compat {
                zone.point_mut(point_ix)?.x += dx;
            }
            if do_touch {
                zone.touch(point_ix, CoordAxis::X)?;
            }
        }
        if fv.y != 0 {
            if !back_compat_and_did_iup {
                zone.point_mut(point_ix)?.y += dy;
            }
            if do_touch {
                zone.touch(point_ix, CoordAxis::Y)?;
            }
        }
        Ok(())
    }

    /// Computes the adjustment made to a point along the current freedom vector.
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5126>
    pub(crate) fn point_displacement(
        &mut self,
        opcode: u8,
    ) -> Result<PointDisplacement, HintErrorKind> {
        let (zone, point_ix) = if (opcode & 1) != 0 {
            (self.zp0, self.rp1)
        } else {
            (self.zp1, self.rp2)
        };
        let zone_data = self.zone(zone);
        let point = zone_data.point(point_ix)?;
        let original_point = zone_data.original(point_ix)?;
        let distance = self.project(point, original_point);
        let fv = self.freedom_vector;
        let dx = math::mul_div(distance, fv.x, self.fdotp);
        let dy = math::mul_div(distance, fv.y, self.fdotp);
        Ok(PointDisplacement {
            zone,
            point_ix,
            dx,
            dy,
        })
    }
}

pub(crate) struct PointDisplacement {
    pub zone: ZonePointer,
    pub point_ix: usize,
    pub dx: i32,
    pub dy: i32,
}

impl CoordAxis {
    fn touched_marker(self) -> PointMarker {
        match self {
            CoordAxis::Both => PointMarker::TOUCHED,
            CoordAxis::X => PointMarker::TOUCHED_X,
            CoordAxis::Y => PointMarker::TOUCHED_Y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CoordAxis, Zone};
    use raw::{
        tables::glyf::{PointFlags, PointMarker},
        types::Point,
    };

    #[test]
    fn flip_on_curve_point() {
        let on_curve = PointFlags::on_curve();
        let off_curve = PointFlags::off_curve_quad();
        let mut zone = Zone {
            unscaled: &mut [],
            original: &mut [],
            points: &mut [],
            contours: &[],
            flags: &mut [on_curve, off_curve, off_curve, on_curve],
        };
        for i in 0..4 {
            zone.flip_on_curve(i).unwrap();
        }
        assert_eq!(zone.flags, &[off_curve, on_curve, on_curve, off_curve]);
    }

    #[test]
    fn set_on_curve_regions() {
        let on_curve = PointFlags::on_curve();
        let off_curve = PointFlags::off_curve_quad();
        let mut zone = Zone {
            unscaled: &mut [],
            original: &mut [],
            points: &mut [],
            contours: &[],
            flags: &mut [on_curve, off_curve, off_curve, on_curve],
        };
        zone.set_on_curve(0, 2, true).unwrap();
        zone.set_on_curve(2, 4, false).unwrap();
        assert_eq!(zone.flags, &[on_curve, on_curve, off_curve, off_curve]);
    }

    #[test]
    fn iup_shift() {
        let [untouched, touched] = point_markers();
        // A single touched point shifts the whole contour
        let mut zone = Zone {
            unscaled: &mut [],
            original: &mut [Point::new(0, 0), Point::new(10, 10), Point::new(20, 20)],
            points: &mut [Point::new(-5, -20), Point::new(10, 10), Point::new(20, 20)],
            contours: &[3],
            flags: &mut [touched, untouched, untouched],
        };
        zone.iup(CoordAxis::X).unwrap();
        assert_eq!(
            zone.points,
            &[Point::new(-5, -20), Point::new(5, 10), Point::new(15, 20)]
        );
        zone.iup(CoordAxis::Y).unwrap();
        assert_eq!(
            zone.points,
            &[Point::new(-5, -20), Point::new(5, -10), Point::new(15, 0)]
        );
    }

    #[test]
    fn iup_interpolate() {
        let [untouched, touched] = point_markers();
        // Two touched points interpolates the intermediate point(s)
        let mut zone = Zone {
            unscaled: &mut [
                Point::new(0, 0),
                Point::new(500, 500),
                Point::new(1000, 1000),
            ],
            original: &mut [Point::new(0, 0), Point::new(10, 10), Point::new(20, 20)],
            points: &mut [Point::new(-5, -20), Point::new(10, 10), Point::new(27, 56)],
            contours: &[3],
            flags: &mut [touched, untouched, touched],
        };
        zone.iup(CoordAxis::X).unwrap();
        assert_eq!(
            zone.points,
            &[Point::new(-5, -20), Point::new(11, 10), Point::new(27, 56)]
        );
        zone.iup(CoordAxis::Y).unwrap();
        assert_eq!(
            zone.points,
            &[Point::new(-5, -20), Point::new(11, 18), Point::new(27, 56)]
        );
    }

    fn point_markers() -> [PointFlags; 2] {
        let untouched = PointFlags::default();
        let mut touched = untouched;
        touched.set_marker(PointMarker::TOUCHED);
        [untouched, touched]
    }
}
