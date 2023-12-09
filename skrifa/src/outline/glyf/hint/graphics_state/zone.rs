//! Glyph zones.

use read_fonts::{
    tables::glyf::{PointFlags, PointMarker},
    types::Point,
};

use super::{
    super::{
        error::HintErrorKind,
        graphics_state::CoordAxis,
        math::{div, mul},
    },
    GraphicsState,
};

use HintErrorKind::{InvalidPointIndex, InvalidPointRange};

/// Reference to either the twilight or glyph zone.
///
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructing_glyphs#zones>
#[derive(Copy, Clone, PartialEq, Default, Debug)]
#[repr(u8)]
pub enum Zone {
    Twilight = 0,
    #[default]
    Glyph = 1,
}

impl Zone {
    pub fn is_twilight(self) -> bool {
        self == Self::Twilight
    }

    pub fn is_glyph(self) -> bool {
        self == Self::Glyph
    }
}

impl TryFrom<i32> for Zone {
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
pub struct ZoneData<'a> {
    pub unscaled: &'a mut [Point<i32>],
    pub original: &'a mut [Point<i32>],
    pub points: &'a mut [Point<i32>],
    pub flags: &'a mut [PointFlags],
    pub contours: &'a [u16],
}

impl<'a> ZoneData<'a> {
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
        let marker = match axis {
            CoordAxis::Both => PointMarker::TOUCHED,
            CoordAxis::X => PointMarker::TOUCHED_X,
            CoordAxis::Y => PointMarker::TOUCHED_Y,
        };
        flag.set_marker(marker);
        Ok(())
    }

    pub fn untouch(&mut self, index: usize, axis: CoordAxis) -> Result<(), HintErrorKind> {
        let flag = self.flags.get_mut(index).ok_or(InvalidPointIndex(index))?;
        let marker = match axis {
            CoordAxis::Both => PointMarker::TOUCHED,
            CoordAxis::X => PointMarker::TOUCHED_X,
            CoordAxis::Y => PointMarker::TOUCHED_Y,
        };
        flag.clear_marker(marker);
        Ok(())
    }

    pub fn is_touched(&self, index: usize, axis: CoordAxis) -> Result<bool, HintErrorKind> {
        let flag = self.flags.get(index).ok_or(InvalidPointIndex(index))?;
        let marker = match axis {
            CoordAxis::Both => PointMarker::TOUCHED,
            CoordAxis::X => PointMarker::TOUCHED_X,
            CoordAxis::Y => PointMarker::TOUCHED_Y,
        };
        Ok(flag.has_marker(marker))
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

    pub fn iup(&mut self, is_x: bool) -> Result<(), HintErrorKind> {
        let coord_axis = if is_x { CoordAxis::X } else { CoordAxis::Y };
        let mut point = 0;
        for i in 0..self.contours.len() {
            let mut end_point = self.contour(i)? as usize;
            let first_point = point;
            if end_point >= self.points.len() {
                end_point = self.points.len() - 1;
            }
            while point <= end_point && !self.is_touched(point, coord_axis)? {
                point += 1;
            }
            if point <= end_point {
                let first_touched = point;
                let mut cur_touched = point;
                point += 1;
                while point <= end_point {
                    if self.is_touched(point, coord_axis)? {
                        self.iup_interpolate(is_x, cur_touched + 1, point - 1, cur_touched, point)?;
                        cur_touched = point;
                    }
                    point += 1;
                }
                if cur_touched == first_touched {
                    self.iup_shift(is_x, first_point, end_point, cur_touched)?;
                } else {
                    self.iup_interpolate(
                        is_x,
                        cur_touched + 1,
                        end_point,
                        cur_touched,
                        first_touched,
                    )?;
                    if first_touched > 0 {
                        self.iup_interpolate(
                            is_x,
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

    fn iup_shift(
        &mut self,
        is_x: bool,
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
        if is_x {
            shift_coord!(x);
        } else {
            shift_coord!(y);
        }
        Ok(())
    }

    fn iup_interpolate(
        &mut self,
        is_x: bool,
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
                    let scale = div(cur2 - cur1, orus2 - orus1);
                    for ((orig, unscaled), point) in iter {
                        let a = orig.$coord;
                        point.$coord = if a <= org1 {
                            a + delta1
                        } else if a >= org2 {
                            a + delta2
                        } else {
                            cur1 + mul(unscaled.$coord - orus1, scale)
                        };
                    }
                }
            };
        }
        if is_x {
            interpolate_coord!(x);
        } else {
            interpolate_coord!(y);
        }
        Ok(())
    }
}

impl<'a> GraphicsState<'a> {
    pub fn reset_zone_pointers(&mut self) {
        self.zp0 = Zone::default();
        self.zp1 = Zone::default();
        self.zp2 = Zone::default();
    }

    #[inline(always)]
    pub fn zone(&self, zone: Zone) -> &ZoneData<'a> {
        &self.zone_data[zone as usize]
    }

    #[inline(always)]
    pub fn zone_mut(&mut self, zone: Zone) -> &mut ZoneData<'a> {
        &mut self.zone_data[zone as usize]
    }

    #[inline(always)]
    pub fn zp0(&self) -> &ZoneData<'a> {
        self.zone(self.zp0)
    }

    #[inline(always)]
    pub fn zp0_mut(&mut self) -> &mut ZoneData<'a> {
        self.zone_mut(self.zp0)
    }

    #[inline(always)]
    pub fn zp1(&self) -> &ZoneData {
        self.zone(self.zp1)
    }

    #[inline(always)]
    pub fn zp1_mut(&mut self) -> &mut ZoneData<'a> {
        self.zone_mut(self.zp1)
    }

    #[inline(always)]
    pub fn zp2(&self) -> &ZoneData {
        self.zone(self.zp2)
    }

    #[inline(always)]
    pub fn zp2_mut(&mut self) -> &mut ZoneData<'a> {
        self.zone_mut(self.zp2)
    }
}
