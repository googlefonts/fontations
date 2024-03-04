//! Managing outlines.
//!
//! Implements 17 instructions.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#managing-outlines>

use super::{
    super::{
        graphics_state::{CoordAxis, PointDisplacement, ZonePointer},
        math,
    },
    Engine, HintErrorKind, OpResult,
};

impl<'a> Engine<'a> {
    /// Flip point.
    ///
    /// FLIPPT[] (0x80)
    ///
    /// Pops: p: point number (uint32)
    ///
    /// Uses the loop counter.
    ///
    /// Flips points that are off the curve so that they are on the curve and
    /// points that are on the curve so that they are off the curve. The point
    /// is not marked as touched. The result of a FLIPPT instruction is that
    /// the contour describing part of a glyph outline is redefined.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#flip-point>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5002>
    pub(super) fn op_flippt(&mut self) -> OpResult {
        let count = self.graphics_state.loop_counter as usize;
        self.graphics_state.loop_counter = 1;
        // In backward compatibility mode, don't flip points after IUP has
        // been done.
        if self.graphics_state.backward_compatibility
            && self.graphics_state.did_iup_x
            && self.graphics_state.did_iup_y
        {
            for _ in 0..count {
                self.value_stack.pop()?;
            }
            return Ok(());
        }
        let zone = self.graphics_state.zone_mut(ZonePointer::Glyph);
        for _ in 0..count {
            let p = self.value_stack.pop_usize()?;
            zone.flip_on_curve(p)?;
        }
        Ok(())
    }

    /// Flip range on.
    ///
    /// FLIPRGON[] (0x81)
    ///
    /// Pops: highpoint: highest point number in range of points to be flipped (uint32)
    ///       lowpoint: lowest point number in range of points to be flipped (uint32)
    ///
    /// Flips a range of points beginning with lowpoint and ending with highpoint so that
    /// any off the curve points become on the curve points. The points are not marked as
    /// touched.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#flip-range-on>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5056>
    pub(super) fn op_fliprgon(&mut self) -> OpResult {
        self.set_on_curve_for_range(true)
    }

    /// Flip range off.
    ///
    /// FLIPRGOFF[] (0x82)
    ///
    /// Pops: highpoint: highest point number in range of points to be flipped (uint32)
    ///       lowpoint: lowest point number in range of points to be flipped (uint32)
    ///
    /// Flips a range of points beginning with lowpoint and ending with
    /// highpoint so that any on the curve points become off the curve points.
    /// The points are not marked as touched.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#flip-range-off>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5094>
    pub(super) fn op_fliprgoff(&mut self) -> OpResult {
        self.set_on_curve_for_range(false)
    }

    /// Shift point by the last point.
    ///
    /// SHP\[a\] (0x32 - 0x33)
    ///
    /// a: 0: uses rp2 in the zone pointed to by zp1
    ///    1: uses rp1 in the zone pointed to by zp0
    ///
    /// Pops: p: point to be shifted
    ///
    /// Uses the loop counter.
    ///
    /// Shift point p by the same amount that the reference point has been
    /// shifted. Point p is shifted along the freedom_vector so that the
    /// distance between the new position of point p and the current position
    /// of point p is the same as the distance between the current position
    /// of the reference point and the original position of the reference point.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#shift-point-by-the-last-point>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5211>
    pub(super) fn op_shp(&mut self, opcode: u8) -> OpResult {
        let gs = &mut self.graphics_state;
        let PointDisplacement { dx, dy, .. } = gs.point_displacement(opcode)?;
        let count = gs.loop_counter;
        gs.loop_counter = 1;
        for _ in 0..count {
            let p = self.value_stack.pop_usize()?;
            gs.move_zp2_point(p, dx, dy, true)?;
        }
        Ok(())
    }

    /// Shift contour by the last point.
    ///
    /// SHC\[a\] (0x34 - 0x35)
    ///
    /// a: 0: uses rp2 in the zone pointed to by zp1
    ///    1: uses rp1 in the zone pointed to by zp0
    ///
    /// Pops: c: contour to be shifted
    ///
    /// Shifts every point on contour c by the same amount that the reference
    /// point has been shifted. Each point is shifted along the freedom_vector
    /// so that the distance between the new position of the point and the old
    /// position of that point is the same as the distance between the current
    /// position of the reference point and the original position of the
    /// reference point. The distance is measured along the projection_vector.
    /// If the reference point is one of the points defining the contour, the
    /// reference point is not moved by this instruction.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#shift-contour-by-the-last-point>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5266>
    pub(super) fn op_shc(&mut self, opcode: u8) -> OpResult {
        let gs = &mut self.graphics_state;
        let contour_ix = self.value_stack.pop_usize()?;
        let point_disp = gs.point_displacement(opcode)?;
        let start = if contour_ix != 0 {
            gs.zp2().contour(contour_ix - 1)? as usize + 1
        } else {
            0
        };
        let end = if gs.zp2.is_twilight() {
            gs.zp2().points.len()
        } else {
            gs.zp2().contour(contour_ix)? as usize + 1
        };
        for i in start..end {
            if point_disp.zone != gs.zp2 || point_disp.point_ix != i {
                gs.move_zp2_point(i, point_disp.dx, point_disp.dy, true)?;
            }
        }
        Ok(())
    }

    /// Shift zone by the last point.
    ///
    /// SHZ\[a\] (0x36 - 0x37)
    ///
    /// a: 0: uses rp2 in the zone pointed to by zp1
    ///    1: uses rp1 in the zone pointed to by zp0
    ///
    /// Pops: e: zone to be shifted
    ///
    /// Shift the points in the specified zone (Z1 or Z0) by the same amount
    /// that the reference point has been shifted. The points in the zone are
    /// shifted along the freedom_vector so that the distance between the new
    /// position of the shifted points and their old position is the same as
    /// the distance between the current position of the reference point and
    /// the original position of the reference point.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#shift-zone-by-the-last-pt>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5318>
    pub(super) fn op_shz(&mut self, opcode: u8) -> OpResult {
        let _e = ZonePointer::try_from(self.value_stack.pop()?)?;
        let gs = &mut self.graphics_state;
        let point_disp = gs.point_displacement(opcode)?;
        let end = if gs.zp2.is_twilight() {
            gs.zp2().points.len()
        } else if !gs.zp2().contours.is_empty() {
            *gs.zp2()
                .contours
                .last()
                .ok_or(HintErrorKind::InvalidContourIndex(0))? as usize
                + 1
        } else {
            0
        };
        for i in 0..end {
            if point_disp.zone != gs.zp2 || i != point_disp.point_ix {
                gs.move_zp2_point(i, point_disp.dx, point_disp.dy, false)?;
            }
        }
        Ok(())
    }

    /// Shift point by a pixel amount.
    ///
    /// SHPIX (0x38)
    ///
    /// Pops: amount: magnitude of the shift (F26Dot6)
    ///       p1, p2,.. pn: points to be shifted
    ///
    /// Uses the loop counter.
    ///
    /// Shifts the points specified by the amount stated. When the loop
    /// variable is used, the amount to be shifted is put onto the stack
    /// only once. That is, if loop = 3, then the contents of the top of
    /// the stack should be point p1, point p2, point p3, amount. The value
    /// amount is expressed in sixty-fourths of a pixel.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#shift-point-by-a-pixel-amount>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5366>
    pub(super) fn op_shpix(&mut self) -> OpResult {
        let gs = &mut self.graphics_state;
        let in_twilight = gs.zp0.is_twilight() || gs.zp1.is_twilight() || gs.zp2.is_twilight();
        let amount = self.value_stack.pop()?;
        let dx = math::mul14(amount, gs.freedom_vector.x);
        let dy = math::mul14(amount, gs.freedom_vector.y);
        let count = gs.loop_counter;
        gs.loop_counter = 1;
        let did_iup = gs.did_iup_x && gs.did_iup_y;
        for _ in 0..count {
            let p = self.value_stack.pop_usize()?;
            if gs.backward_compatibility {
                if in_twilight
                    || (!did_iup
                        && ((self.is_composite && gs.freedom_vector.y != 0)
                            || gs.zp2().is_touched(p, CoordAxis::Y)?))
                {
                    gs.move_zp2_point(p, dx, dy, true)?;
                }
            } else {
                gs.move_zp2_point(p, dx, dy, true)?;
            }
        }
        Ok(())
    }

    /// Move stack indirect relative point.
    ///
    /// MSIRP\[a\] (0x3A - 0x3B)
    ///
    /// a: 0: do not set rp0 to p
    ///    1: set rp0 to p
    ///
    /// Pops: d: distance (F26Dot6)
    ///       p: point number
    ///
    /// Makes the distance between a point p and rp0 equal to the value
    /// specified on the stack. The distance on the stack is in fractional
    /// pixels (F26Dot6). An MSIRP has the same effect as a MIRP instruction
    /// except that it takes its value from the stack rather than the Control
    /// Value Table. As a result, the cut_in does not affect the results of a
    /// MSIRP. Additionally, MSIRP is unaffected by the round_state.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#move-stack-indirect-relative-point>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5439>
    pub(super) fn op_msirp(&mut self, opcode: u8) -> OpResult {
        let gs = &mut self.graphics_state;
        let distance = self.value_stack.pop()?;
        let point_ix = self.value_stack.pop_usize()?;
        if gs.zp1.is_twilight() {
            *gs.zp1_mut().point_mut(point_ix)? = gs.zp0().original(gs.rp0)?;
            gs.move_original(gs.zp1, point_ix, distance)?;
            *gs.zp1_mut().point_mut(point_ix)? = gs.zp1().original(point_ix)?;
        }
        let d = gs.project(gs.zp1().point(point_ix)?, gs.zp0().point(gs.rp0)?);
        gs.move_point(gs.zp1, point_ix, distance.wrapping_sub(d))?;
        gs.rp1 = gs.rp0;
        gs.rp2 = point_ix;
        if (opcode & 1) != 0 {
            gs.rp0 = point_ix;
        }
        Ok(())
    }

    /// Move direct absolute point.
    ///
    /// MDAP\[a\] (0x2E - 0x2F)
    ///
    /// a: 0: do not round the value
    ///    1: round the value
    ///
    /// Pops: p: point number
    ///
    /// Sets the reference points rp0 and rp1 equal to point p. If a=1, this
    /// instruction rounds point p to the grid point specified by the state
    /// variable round_state. If a=0, it simply marks the point as touched in
    /// the direction(s) specified by the current freedom_vector. This command
    /// is often used to set points in the twilight zone.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#move-direct-absolute-point>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5487>
    pub(super) fn op_mdap(&mut self, opcode: u8) -> OpResult {
        let gs = &mut self.graphics_state;
        let p = self.value_stack.pop_usize()?;
        let distance = if (opcode & 1) != 0 {
            let cur_dist = gs.project(gs.zp0().point(p)?, Default::default());
            gs.round(cur_dist) - cur_dist
        } else {
            0
        };
        gs.move_point(gs.zp0, p, distance)?;
        gs.rp0 = p;
        gs.rp1 = p;
        Ok(())
    }

    /// Move indirect absolute point.
    ///
    /// MIAP\[a\] (0x3E - 0x3F)
    ///
    /// a: 0: do not round the distance and don't use control value cutin
    ///    1: round the distance and use control value cutin
    ///
    /// Pops: n: CVT entry number
    ///       p: point number
    ///
    /// Moves point p to the absolute coordinate position specified by the nth
    /// Control Value Table entry. The coordinate is measured along the current
    /// projection_vector. If a=1, the position will be rounded as specified by
    /// round_state. If a=1, and if the device space difference between the CVT
    /// value and the original position is greater than the
    /// control_value_cut_in, then the original position will be rounded
    /// (instead of the CVT value.)
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#move-indirect-absolute-point>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5526>
    pub(super) fn op_miap(&mut self, opcode: u8) -> OpResult {
        let gs = &mut self.graphics_state;
        let cvt_entry = self.value_stack.pop_usize()?;
        let point_ix = self.value_stack.pop_usize()?;
        let mut distance = self.cvt.get(cvt_entry)?;
        if gs.zp0.is_twilight() {
            // Special behavior for twilight zone.
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L5548>
            let fv = gs.freedom_vector;
            let z = gs.zp0_mut();
            let original_point = z.original_mut(point_ix)?;
            original_point.x = math::mul14(distance, fv.x);
            original_point.y = math::mul14(distance, fv.y);
            *z.point_mut(point_ix)? = *original_point;
        }
        let original_distance = gs.project(gs.zp0().point(point_ix)?, Default::default());
        if (opcode & 1) != 0 {
            let delta = (distance.wrapping_sub(original_distance)).abs();
            if delta > gs.control_value_cutin {
                distance = original_distance;
            }
            distance = gs.round(distance);
        }
        gs.move_point(gs.zp0, point_ix, distance.wrapping_sub(original_distance))?;
        gs.rp0 = point_ix;
        gs.rp1 = point_ix;
        Ok(())
    }

    /// Untouch point.
    ///
    /// UTP[] (0x29)
    ///
    /// Pops: p: point number (uint32)
    ///
    /// Marks point p as untouched. A point may be touched in the x direction,
    /// the y direction, both, or neither. This instruction uses the current
    /// freedom_vector to determine whether to untouch the point in the
    /// x-direction, the y direction, or both. Points that are marked as
    /// untouched will be moved by an IUP (interpolate untouched points)
    /// instruction. Using UTP you can ensure that a point will be affected
    /// by IUP even if it was previously touched.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#untouch-point>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6222>
    pub(super) fn op_utp(&mut self) -> OpResult {
        let p = self.value_stack.pop_usize()?;
        let coord_axis = match (
            self.graphics_state.freedom_vector.x != 0,
            self.graphics_state.freedom_vector.y != 0,
        ) {
            (true, true) => Some(CoordAxis::Both),
            (true, false) => Some(CoordAxis::X),
            (false, true) => Some(CoordAxis::Y),
            (false, false) => None,
        };
        if let Some(coord_axis) = coord_axis {
            self.graphics_state.zp0_mut().untouch(p, coord_axis)?;
        }
        Ok(())
    }

    /// Helper for FLIPRGON and FLIPRGOFF.
    fn set_on_curve_for_range(&mut self, on: bool) -> OpResult {
        // high_point is inclusive but Zone::set_on_curve takes an exclusive
        // range
        let high_point = self.value_stack.pop_usize()? + 1;
        let low_point = self.value_stack.pop_usize()?;
        // In backward compatibility mode, don't flip points after IUP has
        // been done.
        if self.graphics_state.backward_compatibility
            && self.graphics_state.did_iup_x
            && self.graphics_state.did_iup_y
        {
            return Ok(());
        }
        self.graphics_state
            .zone_mut(ZonePointer::Glyph)
            .set_on_curve(low_point, high_point, on)
    }
}

#[cfg(test)]
mod tests {
    use raw::tables::glyf::PointMarker;

    use super::{super::MockEngine, CoordAxis};

    #[test]
    fn flip_point() {
        let mut mock = MockEngine::new();
        let mut engine = mock.engine();
        // Points all start as off-curve in the mock engine.
        // Flip every odd point in the first 10
        let count = 5;
        // First, set the loop counter:
        engine.value_stack.push(count).unwrap();
        engine.op_sloop().unwrap();
        // Now push the point indices
        for i in (1..=9).step_by(2) {
            engine.value_stack.push(i).unwrap();
        }
        assert_eq!(engine.value_stack.len(), count as usize);
        // And flip!
        engine.op_flippt().unwrap();
        let flags = &engine.graphics_state.zones[1].flags;
        for i in 0..10 {
            // Odd points are now on-curve
            assert_eq!(flags[i].is_on_curve(), i & 1 != 0);
        }
    }

    /// Backward compat + IUP state prevents flipping.
    #[test]
    fn state_prevents_flip_point() {
        let mut mock = MockEngine::new();
        let mut engine = mock.engine();
        // Points all start as off-curve in the mock engine.
        // Flip every odd point in the first 10
        let count = 5;
        // First, set the loop counter:
        engine.value_stack.push(count).unwrap();
        engine.op_sloop().unwrap();
        // Now push the point indices
        for i in (1..=9).step_by(2) {
            engine.value_stack.push(i).unwrap();
        }
        assert_eq!(engine.value_stack.len(), count as usize);
        // Prevent flipping
        engine.graphics_state.backward_compatibility = true;
        engine.graphics_state.did_iup_x = true;
        engine.graphics_state.did_iup_y = true;
        // But try anyway
        engine.op_flippt().unwrap();
        let flags = &engine.graphics_state.zones[1].flags;
        for i in 0..10 {
            // All points are still off-curve
            assert!(!flags[i].is_on_curve());
        }
    }

    #[test]
    fn flip_range_on_off() {
        let mut mock = MockEngine::new();
        let mut engine = mock.engine();
        // Points all start as off-curve in the mock engine.
        // Flip 10..=20 on
        engine.value_stack.push(10).unwrap();
        engine.value_stack.push(20).unwrap();
        engine.op_fliprgon().unwrap();
        for (i, flag) in engine.graphics_state.zones[1].flags.iter().enumerate() {
            assert_eq!(flag.is_on_curve(), (10..=20).contains(&i));
        }
        // Now flip 12..=15 off
        engine.value_stack.push(12).unwrap();
        engine.value_stack.push(15).unwrap();
        engine.op_fliprgoff().unwrap();
        for (i, flag) in engine.graphics_state.zones[1].flags.iter().enumerate() {
            assert_eq!(
                flag.is_on_curve(),
                (10..=11).contains(&i) || (16..=20).contains(&i)
            );
        }
    }

    /// Backward compat + IUP state prevents flipping.
    #[test]
    fn state_prevents_flip_range_on_off() {
        let mut mock = MockEngine::new();
        let mut engine = mock.engine();
        // Prevent flipping
        engine.graphics_state.backward_compatibility = true;
        engine.graphics_state.did_iup_x = true;
        engine.graphics_state.did_iup_y = true;
        // Points all start as off-curve in the mock engine.
        // Try to flip 10..=20 on
        engine.value_stack.push(10).unwrap();
        engine.value_stack.push(20).unwrap();
        engine.op_fliprgon().unwrap();
        for flag in engine.graphics_state.zones[1].flags.iter() {
            assert!(!flag.is_on_curve());
        }
        // Reset all points to on
        for flag in engine.graphics_state.zones[1].flags.iter_mut() {
            flag.set_on_curve();
        }
        // Now try to flip 12..=15 off
        engine.value_stack.push(12).unwrap();
        engine.value_stack.push(15).unwrap();
        engine.op_fliprgoff().unwrap();
        for flag in engine.graphics_state.zones[1].flags.iter() {
            assert!(flag.is_on_curve());
        }
    }

    #[test]
    fn untouch_point() {
        let mut mock = MockEngine::new();
        let mut engine = mock.engine();
        // Touch all points in both axes to start.
        let count = engine.graphics_state.zones[1].points.len();
        for i in 0..count {
            engine.graphics_state.zones[1]
                .touch(i, CoordAxis::Both)
                .unwrap();
        }
        let mut untouch = |point_ix: usize, fx, fy, marker| {
            assert!(engine.graphics_state.zp0().flags[point_ix].has_marker(marker));
            // Untouch axis is based on freedom vector:
            engine.graphics_state.freedom_vector.x = fx;
            engine.graphics_state.freedom_vector.y = fy;
            engine.value_stack.push(point_ix as i32).unwrap();
            engine.op_utp().unwrap();
            assert!(!engine.graphics_state.zp0().flags[point_ix].has_marker(marker));
        };
        // Untouch point 0 in x axis
        untouch(0, 1, 0, PointMarker::TOUCHED_X);
        // Untouch point 1 in y axis
        untouch(1, 0, 1, PointMarker::TOUCHED_Y);
        // untouch point 2 in both axes
        untouch(2, 1, 1, PointMarker::TOUCHED);
    }
}
