//! Managing outlines.
//!
//! Implements 4 instructions.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#managing-outlines>

use super::{
    super::graphics_state::{CoordAxis, ZonePointer},
    Engine, OpResult,
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
        self.do_fliprg(true)
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
        self.do_fliprg(false)
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
    fn do_fliprg(&mut self, on: bool) -> OpResult {
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
