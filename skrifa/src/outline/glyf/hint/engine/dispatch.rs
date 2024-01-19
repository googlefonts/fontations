use super::{
    super::math::*, CallRecord, CodeDefinition, CoordAxis, Decoder, Engine, HintErrorKind,
    Instruction, Point, PointDisplacement, Program, RoundMode, ZonePointer,
};

impl<'a> Engine<'a> {
    pub(super) fn dispatch(
        &mut self,
        programs: &[&'a [u8]; 3],
        program: Program,
        decoder: &mut Decoder<'a>,
        ins: &Instruction<'a>,
    ) -> Result<(), HintErrorKind> {
        use super::super::code::opcodes as op;
        let opcode = ins.opcode;
        match opcode {
            op::SVTCA0..=op::SFVTCA1 => {
                let aa = (opcode as i32 & 1) << 14;
                let bb = aa ^ 0x4000;
                if opcode < 4 {
                    self.graphics.proj_vector = Point::new(aa, bb);
                    self.graphics.dual_proj_vector = self.graphics.proj_vector;
                }
                if (opcode & 2) == 0 {
                    self.graphics.freedom_vector = Point::new(aa, bb);
                }
                self.graphics.update_projection_state();
            }
            op::SPVTL0..=op::SFVTL1 => {
                let index1 = self.value_stack.pop()? as usize;
                let index2 = self.value_stack.pop()? as usize;
                let p1 = self.graphics.zp1().point(index2)?;
                let p2 = self.graphics.zp2().point(index1)?;
                let mut a = p1.x - p2.x;
                let mut b = p1.y - p2.y;
                let mut op = opcode;
                if a == 0 && b == 0 {
                    a = 0x4000;
                    op = 0;
                }
                if (op & 1) != 0 {
                    let c = b;
                    b = a;
                    a = -c;
                }
                let v = normalize14(a, b);
                if opcode <= op::SPVTL1 {
                    self.graphics.proj_vector = v;
                    self.graphics.dual_proj_vector = v;
                } else {
                    self.graphics.freedom_vector = v;
                }
                self.graphics.update_projection_state();
            }
            op::SPVFS => {
                let y = self.value_stack.pop()? as i16 as i32;
                let x = self.value_stack.pop()? as i16 as i32;
                let v = normalize14(x, y);
                self.graphics.proj_vector = v;
                self.graphics.dual_proj_vector = v;
                self.graphics.update_projection_state();
            }
            op::SFVFS => {
                let y = self.value_stack.pop()? as i16 as i32;
                let x = self.value_stack.pop()? as i16 as i32;
                let v = normalize14(x, y);
                self.graphics.freedom_vector = v;
                self.graphics.update_projection_state();
            }
            op::GPV => {
                self.value_stack.push(self.graphics.proj_vector.x)?;
                self.value_stack.push(self.graphics.proj_vector.y)?;
            }
            op::GFV => {
                self.value_stack.push(self.graphics.freedom_vector.x)?;
                self.value_stack.push(self.graphics.freedom_vector.y)?;
            }
            op::SFVTPV => {
                self.graphics.freedom_vector = self.graphics.proj_vector;
                self.graphics.update_projection_state();
            }
            op::ISECT => {
                let b1 = self.value_stack.pop()? as usize;
                let b0 = self.value_stack.pop()? as usize;
                let a1 = self.value_stack.pop()? as usize;
                let a0 = self.value_stack.pop()? as usize;
                let point_ix = self.value_stack.pop()? as usize;
                let (pa0, pa1) = {
                    let z = self.graphics.zp1();
                    (z.point(a0)?, z.point(a1)?)
                };
                let (pb0, pb1) = {
                    let z = self.graphics.zp0();
                    (z.point(b0)?, z.point(b1)?)
                };
                let dbx = pb1.x - pb0.x;
                let dby = pb1.y - pb0.y;
                let dax = pa1.x - pa0.x;
                let day = pa1.y - pa0.y;
                let dx = pb0.x - pa0.x;
                let dy = pb0.y - pa0.y;
                let discriminant = mul_div(dax, -dby, 0x40) + mul_div(day, dbx, 0x40);
                let dp = mul_div(dax, dbx, 0x40) + mul_div(day, dby, 0x40);
                if 19 * discriminant.abs() > dp.abs() {
                    let v = mul_div(dx, -dby, 0x40) + mul_div(dy, dbx, 0x40);
                    let x = mul_div(v, dax, discriminant);
                    let y = mul_div(v, day, discriminant);
                    let point = self.graphics.zp2_mut().point_mut(point_ix)?;
                    point.x = pa0.x + x;
                    point.y = pa0.y + y;
                } else {
                    let point = self.graphics.zp2_mut().point_mut(point_ix)?;
                    point.x = (pa0.x + pa1.x + pb0.x + pb1.x) / 4;
                    point.y = (pa0.y + pa1.y + pb0.y + pb1.y) / 4;
                }
                self.graphics.zp2_mut().touch(point_ix, CoordAxis::Both)?;
            }
            op::SRP0 => self.graphics.rp0 = self.value_stack.pop()? as usize,
            op::SRP1 => self.graphics.rp1 = self.value_stack.pop()? as usize,
            op::SRP2 => self.graphics.rp2 = self.value_stack.pop()? as usize,
            op::SZP0 => {
                let z = self.value_stack.pop()?;
                self.graphics.zp0 = ZonePointer::try_from(z)?;
            }
            op::SZP1 => {
                let z = self.value_stack.pop()?;
                self.graphics.zp1 = ZonePointer::try_from(z)?;
            }
            op::SZP2 => {
                let z = self.value_stack.pop()?;
                self.graphics.zp2 = ZonePointer::try_from(z)?;
            }
            op::SZPS => {
                let z = self.value_stack.pop()?;
                let zp = ZonePointer::try_from(z)?;
                self.graphics.zp0 = zp;
                self.graphics.zp1 = zp;
                self.graphics.zp2 = zp;
            }
            op::SLOOP => {
                let c = self.value_stack.pop()?;
                if c < 0 {
                    return Err(HintErrorKind::NegativeLoopCounter);
                } else {
                    self.graphics.loop_counter = (c as u32).min(0xFFFF);
                }
            }
            op::RTG => self.graphics.round_state.mode = RoundMode::Grid,
            op::RTHG => self.graphics.round_state.mode = RoundMode::HalfGrid,
            op::SMD => self.graphics.min_distance = self.value_stack.pop()?,
            op::ELSE => self.op_else(decoder)?,
            op::SCVTCI => self.graphics.control_value_cutin = self.value_stack.pop()?,
            op::SSWCI => self.graphics.single_width_cutin = self.value_stack.pop()?,
            op::SSW => self.graphics.single_width = self.value_stack.pop()?,
            op::DUP => self.op_dup()?,
            op::POP => self.op_pop()?,
            op::CLEAR => self.op_clear()?,
            op::SWAP => self.op_swap()?,
            op::DEPTH => self.op_depth()?,
            op::CINDEX => self.op_cindex()?,
            op::MINDEX => self.op_mindex()?,
            op::ALIGNPTS => {
                let p2 = self.value_stack.pop_usize()?;
                let p1 = self.value_stack.pop_usize()?;
                let distance = self.graphics.project(
                    self.graphics.zp0().point(p2)?,
                    self.graphics.zp1().point(p1)?,
                ) / 2;
                self.move_point(self.graphics.zp1, p1, distance)?;
                self.move_point(self.graphics.zp0, p2, -distance)?;
            }
            op::UTP => {
                let point_ix = self.value_stack.pop_usize()?;
                let coord_axis = match (
                    self.graphics.freedom_vector.x != 0,
                    self.graphics.freedom_vector.y != 0,
                ) {
                    (true, true) => Some(CoordAxis::Both),
                    (true, false) => Some(CoordAxis::X),
                    (false, true) => Some(CoordAxis::Y),
                    (false, false) => None,
                };
                if let Some(coord_axis) = coord_axis {
                    self.graphics.zp0_mut().untouch(point_ix, coord_axis)?;
                }
            }
            op::LOOPCALL | op::CALL => {
                let (def_ix, call_count) = if opcode == op::LOOPCALL {
                    (self.value_stack.pop_usize()?, self.value_stack.pop()?)
                } else {
                    (self.value_stack.pop_usize()?, 1)
                };
                if call_count > 0 {
                    let def = self.function_defs.get(def_ix)?;
                    if !def.is_active() {
                        return Err(HintErrorKind::InvalidDefintionIndex(def_ix));
                    }
                    let return_pc = ins.pc + 1;
                    let program = decoder.program;
                    let rec = CallRecord {
                        caller_program: program,
                        return_pc,
                        current_count: call_count as u32,
                        definition: def,
                    };
                    self.call_stack.push(rec)?;
                    *decoder = Decoder::new(
                        def.program(),
                        programs[def.program() as usize],
                        def.range().start,
                    );
                }
            }
            op::FDEF => {
                let def_ix = self.value_stack.pop_usize()?;
                if program == Program::Glyph || def_ix >= self.function_defs.len() {
                    return Err(HintErrorKind::DefinitionInGlyphProgram);
                }
                let start = ins.pc + 1;
                while let Some(next_ins) = decoder.maybe_next() {
                    let next_ins = next_ins?;
                    match next_ins.opcode {
                        op::IDEF | op::FDEF => {
                            return Err(HintErrorKind::NestedDefinition);
                        }
                        op::ENDF => {
                            let def = CodeDefinition::new(program, start..decoder.pc, None);
                            self.function_defs.set(def_ix, def)?;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            op::ENDF => {
                let mut rec = self.call_stack.pop()?;
                if rec.current_count > 1 {
                    rec.current_count -= 1;
                    decoder.pc = rec.definition.range().start;
                    self.call_stack.push(rec)?;
                } else {
                    *decoder = Decoder::new(
                        rec.caller_program,
                        programs[rec.caller_program as usize],
                        rec.return_pc,
                    );
                }
            }
            op::MDAP0 | op::MDAP1 => {
                let point_ix = self.value_stack.pop_usize()?;
                let mut distance = 0;
                if (opcode & 1) != 0 {
                    let c = self
                        .graphics
                        .project(self.graphics.zp0().point(point_ix)?, Point::default());
                    distance = self.graphics.round(c) - c;
                }
                self.move_point(self.graphics.zp0, point_ix, distance)?;
                self.graphics.rp0 = point_ix;
                self.graphics.rp1 = point_ix;
            }
            op::IUP0 | op::IUP1 => {
                let axis = if (opcode & 1) != 0 {
                    CoordAxis::X
                } else {
                    CoordAxis::Y
                };
                let mut run = true;
                if !self.is_v35 && self.backward_compat_enabled {
                    if self.did_iup_x && self.did_iup_y {
                        run = false;
                    }
                    if axis == CoordAxis::X {
                        self.did_iup_x = true;
                    } else {
                        self.did_iup_y = true;
                    }
                }
                if run {
                    self.graphics.zone_mut(ZonePointer::Glyph).iup(axis)?;
                }
            }
            op::SHP0 | op::SHP1 => {
                let PointDisplacement { dx, dy, .. } =
                    self.point_displacement(opcode, self.graphics.rp1, self.graphics.rp2)?;
                let mut iters = core::mem::replace(&mut self.graphics.loop_counter, 1);
                while iters > 0 {
                    let index = self.value_stack.pop_usize()?;
                    self.move_zp2_point(index, dx, dy, true)?;
                    iters -= 1;
                }
            }
            op::SHC0 | op::SHC1 => {
                let contour_ix = self.value_stack.pop_usize()?;
                let bound = if self.graphics.zp2 == ZonePointer::Twilight {
                    1
                } else {
                    self.graphics.zp2().contours.len()
                };
                if contour_ix >= bound {
                    return Err(HintErrorKind::InvalidContourIndex(contour_ix));
                }
                let point_disp =
                    self.point_displacement(opcode, self.graphics.rp1, self.graphics.rp2)?;
                let mut start = 0;
                if contour_ix != 0 {
                    let z = self.graphics.zp2();
                    start = z.contour(contour_ix - 1)? as usize + 1;
                }
                let limit = if self.graphics.zp2 == ZonePointer::Twilight {
                    self.graphics.zp2().points.len()
                } else {
                    let z = self.graphics.zp2();
                    z.contour(contour_ix)? as usize + 1
                };
                for i in start..limit {
                    if point_disp.zone != self.graphics.zp2 || point_disp.point_ix != i {
                        self.move_zp2_point(i, point_disp.dx, point_disp.dy, true)?;
                    }
                }
            }
            op::SHZ0 | op::SHZ1 => {
                if self.value_stack.pop()? >= 2 {
                    return Err(HintErrorKind::InvalidStackValue);
                }
                let point_disp =
                    self.point_displacement(opcode, self.graphics.rp1, self.graphics.rp2)?;
                let limit = if self.graphics.zp2 == ZonePointer::Twilight {
                    self.graphics.zp2().points.len()
                } else if self.graphics.zp2 == ZonePointer::Glyph
                    && !self.graphics.zp2().contours.is_empty()
                {
                    let z = self.graphics.zp2();
                    *z.contours
                        .last()
                        .ok_or(HintErrorKind::InvalidContourIndex(0))? as usize
                        + 1
                } else {
                    0
                };
                for i in 0..limit {
                    if point_disp.zone != self.graphics.zp2 || i != point_disp.point_ix {
                        self.move_zp2_point(i, point_disp.dx, point_disp.dy, false)?;
                    }
                }
            }
            op::SHPIX => {
                let in_twilight = self.graphics.zp0 == ZonePointer::Twilight
                    || self.graphics.zp1 == ZonePointer::Twilight
                    || self.graphics.zp2 == ZonePointer::Twilight;
                let a = self.value_stack.pop()?;
                let dx = mul14(a, self.graphics.freedom_vector.x);
                let dy = mul14(a, self.graphics.freedom_vector.y);
                let mut iters = core::mem::replace(&mut self.graphics.loop_counter, 1);
                while iters > 0 {
                    let point = self.value_stack.pop_usize()?;
                    if !self.is_v35 && self.backward_compat_enabled {
                        if in_twilight
                            || (!(self.did_iup_x && self.did_iup_y)
                                && ((self.is_composite && self.graphics.freedom_vector.y != 0)
                                    || self.graphics.zp2().is_touched(point, CoordAxis::Y)?))
                        {
                            self.move_zp2_point(point, dx, dy, true)?;
                        }
                    } else {
                        self.move_zp2_point(point, dx, dy, true)?;
                    }
                    iters -= 1;
                }
            }
            op::IP => {
                let in_twilight = self.graphics.zp0 == ZonePointer::Twilight
                    || self.graphics.zp1 == ZonePointer::Twilight
                    || self.graphics.zp2 == ZonePointer::Twilight;
                let orus_base = if in_twilight {
                    self.graphics.zp0().original(self.graphics.rp1)?
                } else {
                    self.graphics.zp0().unscaled(self.graphics.rp1)?
                };
                let cur_base = self.graphics.zp0().point(self.graphics.rp1)?;
                let old_range = if in_twilight {
                    self.graphics
                        .dual_project(self.graphics.zp1().original(self.graphics.rp2)?, orus_base)
                } else {
                    self.graphics
                        .dual_project(self.graphics.zp1().unscaled(self.graphics.rp2)?, orus_base)
                };
                let cur_range = self
                    .graphics
                    .project(self.graphics.zp1().point(self.graphics.rp2)?, cur_base);
                let mut iters = core::mem::replace(&mut self.graphics.loop_counter, 1);
                while iters > 0 {
                    iters -= 1;
                    let point = self.value_stack.pop_usize()?;
                    let original_distance = if in_twilight {
                        self.graphics
                            .dual_project(self.graphics.zp2().original(point)?, orus_base)
                    } else {
                        self.graphics
                            .dual_project(self.graphics.zp2().unscaled(point)?, orus_base)
                    };
                    let cur_distance = self
                        .graphics
                        .project(self.graphics.zp2().point(point)?, cur_base);
                    let mut new_distance = 0;
                    if original_distance != 0 {
                        if old_range != 0 {
                            new_distance = mul_div(original_distance, cur_range, old_range);
                        } else {
                            new_distance = original_distance;
                        }
                    }
                    self.move_point(self.graphics.zp2, point, new_distance - cur_distance)?;
                }
            }
            op::MSIRP0 | op::MSIRP1 => {
                let dist = self.value_stack.pop()?;
                let point_ix = self.value_stack.pop_usize()?;
                if self.graphics.zp1 == ZonePointer::Twilight {
                    *self.graphics.zp1_mut().point_mut(point_ix)? =
                        self.graphics.zp0().original(self.graphics.rp0)?;
                    self.move_original(self.graphics.zp1, point_ix, dist)?;
                    *self.graphics.zp1_mut().point_mut(point_ix)? =
                        self.graphics.zp1().original(point_ix)?;
                }
                let d = self.graphics.project(
                    self.graphics.zp1().point(point_ix)?,
                    self.graphics.zp0().point(self.graphics.rp0)?,
                );
                self.move_point(self.graphics.zp1, point_ix, dist.wrapping_sub(d))?;
                self.graphics.rp1 = self.graphics.rp0;
                self.graphics.rp2 = point_ix;
                if (opcode & 1) != 0 {
                    self.graphics.rp0 = point_ix;
                }
            }
            op::ALIGNRP => {
                let mut iters = core::mem::replace(&mut self.graphics.loop_counter, 1);
                while iters > 0 {
                    let point = self.value_stack.pop_usize()?;
                    let distance = self.graphics.project(
                        self.graphics.zp1().point(point)?,
                        self.graphics.zp0().point(self.graphics.rp0)?,
                    );
                    self.move_point(self.graphics.zp1, point, -distance)?;
                    iters -= 1;
                }
            }
            op::RTDG => self.graphics.round_state.mode = RoundMode::DoubleGrid,
            op::MIAP0 | op::MIAP1 => {
                let cvt_entry = self.value_stack.pop_usize()?;
                let point_ix = self.value_stack.pop_usize()?;
                let mut distance = self.cvt.get(cvt_entry)?;
                if self.graphics.zp0 == ZonePointer::Twilight {
                    let fv = self.graphics.freedom_vector;
                    let z = self.graphics.zp0_mut();
                    let original_point = z.original_mut(point_ix)?;
                    original_point.x = mul14(distance, fv.x);
                    original_point.y = mul14(distance, fv.y);
                    *z.point_mut(point_ix)? = *original_point;
                }
                let original_distance = self
                    .graphics
                    .project(self.graphics.zp0().point(point_ix)?, Point::default());
                if (opcode & 1) != 0 {
                    let delta = (distance - original_distance).abs();
                    if delta > self.graphics.control_value_cutin {
                        distance = original_distance;
                    }
                    distance = self.graphics.round(distance);
                }
                self.move_point(self.graphics.zp0, point_ix, distance - original_distance)?;
                self.graphics.rp0 = point_ix;
                self.graphics.rp1 = point_ix;
            }
            op::WS => self.op_ws()?,
            op::RS => self.op_rs()?,
            op::WCVTP => self.op_wcvtp()?,
            op::WCVTF => self.op_wcvtf()?,
            op::RCVT => self.op_rcvt()?,
            op::GC0 | op::GC1 => {
                let index = self.value_stack.pop_usize()?;
                let r = if (opcode & 1) != 0 {
                    self.graphics
                        .dual_project(self.graphics.zp2().original(index)?, Point::default())
                } else {
                    self.graphics
                        .project(self.graphics.zp2().point(index)?, Point::default())
                };
                self.value_stack.push(r)?;
            }
            op::SCFS => {
                let distance = self.value_stack.pop()?;
                let point_ix = self.value_stack.pop_usize()?;
                let a = self
                    .graphics
                    .project(self.graphics.zp2().point(point_ix)?, Point::default());
                self.move_point(self.graphics.zp2, point_ix, distance.wrapping_sub(a))?;
                if self.graphics.zp2 == ZonePointer::Twilight {
                    let twilight = self.graphics.zone_mut(ZonePointer::Twilight);
                    *twilight.original_mut(point_ix)? = twilight.point(point_ix)?;
                }
            }
            op::MD0 | op::MD1 => {
                let point1_ix = self.value_stack.pop_usize()?;
                let point2_ix = self.value_stack.pop_usize()?;
                let distance = if (opcode & 1) != 0 {
                    self.graphics.project(
                        self.graphics.zp0().point(point2_ix)?,
                        self.graphics.zp1().point(point1_ix)?,
                    )
                } else if self.graphics.zp0 == ZonePointer::Twilight
                    || self.graphics.zp1 == ZonePointer::Twilight
                {
                    self.graphics.dual_project(
                        self.graphics.zp0().original(point2_ix)?,
                        self.graphics.zp1().original(point1_ix)?,
                    )
                } else {
                    mul(
                        self.graphics.dual_project(
                            self.graphics.zp0().unscaled(point2_ix)?,
                            self.graphics.zp1().unscaled(point1_ix)?,
                        ),
                        self.y_scale,
                    )
                };
                self.value_stack.push(distance)?;
            }
            op::MPPEM => {
                self.value_stack.push(self.instance.ppem as i32)?;
            }
            op::MPS => {
                self.value_stack.push(if self.is_v35 {
                    self.instance.ppem as i32
                } else {
                    mul_div(self.instance.ppem as i32, 64 * 72, 72)
                })?;
            }
            op::FLIPON => self.graphics.auto_flip = true,
            op::FLIPOFF => self.graphics.auto_flip = false,
            op::DEBUG => {}
            op::LT => self.op_lt()?,
            op::LTEQ => self.op_lteq()?,
            op::GT => self.op_gt()?,
            op::GTEQ => self.op_gteq()?,
            op::EQ => self.op_eq()?,
            op::NEQ => self.op_neq()?,
            op::ODD => self.op_odd()?,
            op::EVEN => self.op_even()?,
            op::IF => self.op_if(decoder)?,
            op::EIF => self.op_eif()?,
            op::AND => self.op_and()?,
            op::OR => self.op_or()?,
            op::NOT => self.op_not()?,
            op::SDB => self.graphics.delta_base = self.value_stack.pop()? as u16,
            op::SDS => self.graphics.delta_shift = self.value_stack.pop()?.min(6) as u16,
            op::ADD => self.op_add()?,
            op::SUB => self.op_sub()?,
            op::DIV => self.op_div()?,
            op::MUL => self.op_mul()?,
            op::ABS => self.op_abs()?,
            op::NEG => self.op_neg()?,
            op::FLOOR => self.op_floor()?,
            op::CEILING => self.op_ceiling()?,
            op::ROUND00..=op::ROUND11 => {
                let round_state = self.graphics.round_state;
                self.value_stack.apply_unary(|a| Ok(round_state.round(a)))?;
            }
            op::NROUND00..=op::NROUND11 => {}
            op::DELTAP1 | op::DELTAP2 | op::DELTAP3 => {
                let ppem = self.instance.ppem as u32;
                let num_pairs = self.value_stack.pop_usize()?;
                let bias = match opcode {
                    op::DELTAP2 => 16,
                    op::DELTAP3 => 32,
                    _ => 0,
                } + self.graphics.delta_base as u32;
                for _ in 0..num_pairs {
                    let point_ix = self.value_stack.pop_usize()?;
                    let mut b = self.value_stack.pop()?;
                    if point_ix >= self.graphics.zp0().points.len() {
                        continue;
                    }
                    let mut c = (b as u32 & 0xF0) >> 4;
                    c += bias;
                    if ppem == c {
                        b = (b & 0xF) - 8;
                        if b >= 0 {
                            b += 1;
                        }
                        b *= 1 << (6 - self.graphics.delta_shift as i32);
                        if !self.is_v35 && self.backward_compat_enabled {
                            if !(self.did_iup_x && self.did_iup_y)
                                && ((self.is_composite && self.graphics.freedom_vector.y != 0)
                                    || self.graphics.zp0().is_touched(point_ix, CoordAxis::Y)?)
                            {
                                self.move_point(self.graphics.zp0, point_ix, b)?;
                            }
                        } else {
                            self.move_point(self.graphics.zp0, point_ix, b)?;
                        }
                    }
                }
            }
            op::DELTAC1 | op::DELTAC2 | op::DELTAC3 => {
                let ppem = self.instance.ppem as u32;
                let num_pairs = self.value_stack.pop_usize()?;
                let bias = match opcode {
                    op::DELTAC2 => 16,
                    op::DELTAC3 => 32,
                    _ => 0,
                } + self.graphics.delta_base as u32;
                for _ in 0..num_pairs {
                    let cvt_ix = self.value_stack.pop_usize()?;
                    let mut b = self.value_stack.pop()?;
                    let mut c = (b as u32 & 0xF0) >> 4;
                    c += bias;
                    if ppem == c {
                        b = (b & 0xF) - 8;
                        if b >= 0 {
                            b += 1;
                        }
                        b *= 1 << (6 - self.graphics.delta_shift as i32);
                        let cvt_val = self.cvt.get(cvt_ix)?;
                        self.cvt.set(cvt_ix, cvt_val + b)?;
                    }
                }
            }
            op::SROUND | op::S45ROUND => {
                let selector = self.value_stack.pop()?;
                let grid_period = if opcode == op::SROUND {
                    self.graphics.round_state.mode = RoundMode::Super;
                    0x4000
                } else {
                    self.graphics.round_state.mode = RoundMode::Super45;
                    0x2D41
                };
                let mut period = self.graphics.round_state.period;
                match selector & 0xC0 {
                    0 => period = grid_period / 2,
                    0x40 => period = grid_period,
                    0x80 => period = grid_period * 2,
                    0xC0 => period = grid_period,
                    _ => {}
                }
                self.graphics.round_state.period = period;
                let mut phase = self.graphics.round_state.phase;
                match selector & 0x30 {
                    0 => phase = 0,
                    0x10 => phase = period / 4,
                    0x20 => phase = period / 2,
                    0x30 => phase = period * 3 / 4,
                    _ => {}
                }
                self.graphics.round_state.phase = phase;
                if (selector & 0x0F) == 0 {
                    self.graphics.round_state.threshold = period - 1;
                } else {
                    self.graphics.round_state.threshold = ((selector & 0x0F) - 4) * period / 8;
                }
                self.graphics.round_state.period >>= 8;
                self.graphics.round_state.phase >>= 8;
                self.graphics.round_state.threshold >>= 8;
            }
            op::JROT => self.op_jrot(decoder)?,
            op::JMPR => self.op_jmpr(decoder)?,
            op::JROF => self.op_jrof(decoder)?,
            op::ROFF => self.graphics.round_state.mode = RoundMode::Off,
            op::RUTG => self.graphics.round_state.mode = RoundMode::UpToGrid,
            op::RDTG => self.graphics.round_state.mode = RoundMode::DownToGrid,
            op::SANGW => {}
            op::AA => {}
            op::FLIPPT => {
                if !self.is_v35 && self.backward_compat_enabled && self.did_iup_x && self.did_iup_y
                {
                    // nothing
                } else {
                    let mut iters = core::mem::replace(&mut self.graphics.loop_counter, 1);
                    while iters > 0 {
                        let point = self.value_stack.pop_usize()?;
                        self.graphics
                            .zone_mut(ZonePointer::Glyph)
                            .flip_on_curve(point)?;
                        iters -= 1;
                    }
                }
            }
            op::FLIPRGON | op::FLIPRGOFF => {
                if !self.is_v35 && self.backward_compat_enabled && self.did_iup_x && self.did_iup_y
                {
                    // nothing
                } else {
                    let last_point_ix = self.value_stack.pop_usize()?;
                    let first_point_ix = self.value_stack.pop_usize()?;
                    if first_point_ix > last_point_ix {
                        return Err(HintErrorKind::InvalidPointIndex(first_point_ix));
                    }
                    self.graphics.zone_mut(ZonePointer::Glyph).set_on_curve(
                        first_point_ix,
                        last_point_ix + 1,
                        opcode == op::FLIPRGON,
                    )?;
                }
            }
            op::SCANCTRL => {
                let a = self.value_stack.pop()? as u16;
                let b = a & 0xFF;
                let scan_control = &mut self.graphics.scan_control;
                if b == 0xFF {
                    *scan_control = true;
                } else if b == 0 {
                    *scan_control = false;
                } else {
                    if (a & 0x100) != 0 && self.instance.ppem <= b {
                        *scan_control = true;
                    }
                    if (a & 0x200) != 0 && self.is_rotated {
                        *scan_control = true;
                    }
                    if (a & 0x800) != 0 && self.instance.ppem > b {
                        *scan_control = false;
                    }
                    if (a & 0x1000) != 0 && self.is_rotated {
                        *scan_control = false;
                    }
                }
            }
            op::SDPVTL0 | op::SDPVTL1 => {
                let mut op = opcode;
                let p1 = self.value_stack.pop_usize()?;
                let p2 = self.value_stack.pop_usize()?;
                let mut a;
                let mut b;
                {
                    let v1 = self.graphics.zp1().original(p2)?;
                    let v2 = self.graphics.zp2().original(p1)?;
                    a = v1.x - v2.x;
                    b = v1.y - v2.y;
                    if a == 0 && b == 0 {
                        a = 0x4000;
                        op = 0;
                    }
                }
                if (op & 1) != 0 {
                    let c = b;
                    b = a;
                    a = -c;
                }
                let v = normalize14(a, b);
                self.graphics.dual_proj_vector = v;
                {
                    let v1 = self.graphics.zp1().point(p2)?;
                    let v2 = self.graphics.zp2().point(p1)?;
                    a = v1.x - v2.x;
                    b = v1.y - v2.y;
                    if a == 0 && b == 0 {
                        a = 0x4000;
                        op = 0;
                    }
                }
                if (op & 1) != 0 {
                    let c = b;
                    b = a;
                    a = -c;
                }
                let v = normalize14(a, b);
                self.graphics.proj_vector = v;
                self.graphics.update_projection_state();
            }
            op::GETINFO => {
                // See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#get-information>
                let selector = self.value_stack.pop()?;
                let mut result = 0;
                // Interpreter version
                // selector bit: 1
                // result bits: 0-7
                const VERSION_SELECTOR_BIT: i32 = 1 << 0;
                if (selector & VERSION_SELECTOR_BIT) != 0 {
                    result = if self.is_v35 { 35 } else { 42 };
                }
                // Font variations
                // selector bit: 3
                // result bit: 10
                const FONT_VARIATIONS_SELECTOR_BIT: i32 = 1 << 3;
                const FONT_VARIATIONS_RESULT_MASK: i32 = 1 << 10;
                if (selector & FONT_VARIATIONS_SELECTOR_BIT) != 0 && !self.coords.is_empty() {
                    result |= FONT_VARIATIONS_RESULT_MASK;
                }
                // The following only apply for interpreter version 40
                // and antialiased hinting
                if !self.is_v35 && self.instance.mode.is_antialiased() {
                    // Subpixel hinting (cleartype enabled)
                    // selector bit: 6
                    // result bit: 13
                    // (always enabled)
                    const SUBPIXEL_HINTING_SELECTOR_BIT: i32 = 1 << 6;
                    const SUBPIXEL_HINTING_RESULT_MASK: i32 = 1 << 13;
                    if (selector & SUBPIXEL_HINTING_SELECTOR_BIT) != 0 {
                        result |= SUBPIXEL_HINTING_RESULT_MASK;
                    }
                    // Vertical LCD subpixels?
                    // selector bit: 8
                    // result bit: 15
                    const VERTICAL_LCD_SELECTOR_BIT: i32 = 1 << 8;
                    const VERTICAL_LCD_RESULT_MASK: i32 = 1 << 15;
                    if (selector & VERSION_SELECTOR_BIT) != 0
                        && self.instance.mode.is_vertical_lcd()
                    {
                        result |= VERTICAL_LCD_RESULT_MASK;
                    }
                    // Subpixel positioned?
                    // selector bit: 10
                    // result bit: 17
                    // (always enabled)
                    const SUBPIXEL_POSITIONED_SELECTOR_BIT: i32 = 1 << 10;
                    const SUBPIXEL_POSITIONED_RESULT_MASK: i32 = 1 << 17;
                    if (selector & SUBPIXEL_POSITIONED_SELECTOR_BIT) != 0 {
                        result |= SUBPIXEL_POSITIONED_RESULT_MASK;
                    }
                    // Symmetrical smoothing
                    // selector bit: 11
                    // result bit: 18
                    const SYMMETRICAL_SMOOTHING_SELECTOR_BIT: i32 = 1 << 11;
                    const SYMMETRICAL_SMOOTHING_RESULT_MASK: i32 = 1 << 18;
                    if (selector & SYMMETRICAL_SMOOTHING_SELECTOR_BIT) != 0
                        && !self.instance.mode.retain_linear_metrics()
                    {
                        result |= SYMMETRICAL_SMOOTHING_RESULT_MASK;
                    }
                    // ClearType hinting and grayscale rendering
                    // selector bit: 12
                    // result bit: 19
                    const GRAYSCALE_CLEARTYPE_SELECTOR_BIT: i32 = 1 << 12;
                    const GRAYSCALE_CLEARTYPE_RESULT_MASK: i32 = 1 << 19;
                    if (selector & GRAYSCALE_CLEARTYPE_SELECTOR_BIT) != 0
                        && self.instance.mode.is_grayscale_cleartype()
                    {
                        result |= 1 << 19;
                    }
                }
                self.value_stack.push(result)?;
            }
            op::IDEF => {
                if program == Program::Glyph {
                    return Err(HintErrorKind::DefinitionInGlyphProgram);
                }
                let def_ix = self.value_stack.pop_usize()?;
                let mut index = !0;
                for i in 0..self.instruction_defs.len() {
                    if !self.instruction_defs.get(i)?.is_active() {
                        index = i;
                        break;
                    }
                }
                if index == !0 {
                    return Err(HintErrorKind::InvalidDefintionIndex(
                        self.instruction_defs.len(),
                    ));
                }
                let start = ins.pc + 1;
                while let Some(next_ins) = decoder.maybe_next() {
                    let next_ins = next_ins?;
                    match next_ins.opcode {
                        op::IDEF | op::FDEF => {
                            return Err(HintErrorKind::NestedDefinition);
                        }
                        op::ENDF => {
                            let def =
                                CodeDefinition::new(program, start..decoder.pc, Some(def_ix as u8));
                            self.instruction_defs.set(index, def)?;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            op::ROLL => self.op_roll()?,
            op::MAX => self.op_max()?,
            op::MIN => self.op_min()?,
            op::SCANTYPE => {
                let scan_type = self.value_stack.pop()?;
                if scan_type >= 0 {
                    self.graphics.scan_type = scan_type & 0xFFFF;
                }
            }
            op::INSTCTRL => {
                let selector = self.value_stack.pop()? as u32;
                let value = self.value_stack.pop()? as u32;
                let af = 1 << (selector - 1);
                if !(1..=3).contains(&selector) || (value != 0 && value != af) {
                    // nothing
                } else {
                    self.graphics.instruct_control &= !(af as u8);
                    self.graphics.instruct_control |= value as u8;
                    if selector == 3 && !self.is_v35 && !self.instance.mode.retain_linear_metrics()
                    {
                        self.backward_compat_enabled = value != 4;
                    }
                }
            }
            op::PUSHB000..=op::PUSHW111 | op::NPUSHB | op::NPUSHW => {
                self.op_push(&ins.arguments)?;
            }
            op::MDRP00000..=op::MDRP11111 => {
                let point_ix = self.value_stack.pop_usize()?;
                let mut original_distance;
                if self.graphics.zp0 == ZonePointer::Twilight
                    || self.graphics.zp1 == ZonePointer::Twilight
                {
                    original_distance = self.graphics.dual_project(
                        self.graphics.zp1().original(point_ix)?,
                        self.graphics.zp0().original(self.graphics.rp0)?,
                    );
                } else {
                    let v1 = self.graphics.zp1().unscaled(point_ix)?;
                    let v2 = self.graphics.zp0().unscaled(self.graphics.rp0)?;
                    original_distance = self.graphics.dual_project(v1, v2);
                    original_distance = mul(original_distance, self.y_scale);
                }
                let cutin = self.graphics.single_width_cutin;
                let value = self.graphics.single_width;
                if cutin > 0
                    && original_distance < value + cutin
                    && original_distance > value - cutin
                {
                    original_distance = if original_distance >= 0 {
                        value
                    } else {
                        -value
                    };
                }
                let mut distance = if (opcode & 4) != 0 {
                    self.graphics.round(original_distance)
                } else {
                    original_distance
                };
                let min_distance = self.graphics.min_distance;
                if (opcode & 8) != 0 {
                    if original_distance >= 0 {
                        if distance < min_distance {
                            distance = min_distance;
                        }
                    } else if distance > -min_distance {
                        distance = -min_distance;
                    }
                }
                original_distance = self.graphics.project(
                    self.graphics.zp1().point(point_ix)?,
                    self.graphics.zp0().point(self.graphics.rp0)?,
                );
                self.move_point(
                    self.graphics.zp1,
                    point_ix,
                    distance.wrapping_sub(original_distance),
                )?;
                self.graphics.rp1 = self.graphics.rp0;
                self.graphics.rp2 = point_ix;
                if (opcode & 16) != 0 {
                    self.graphics.rp0 = point_ix;
                }
            }
            op::MIRP00000..=op::MIRP11111 => {
                let cvt_entry = (self.value_stack.pop()? + 1) as usize;
                let point_ix = self.value_stack.pop_usize()?;
                let mut cvt_distance = if cvt_entry == 0 {
                    0
                } else {
                    self.cvt.get(cvt_entry - 1)?
                };
                let cutin = self.graphics.single_width_cutin;
                let value = self.graphics.single_width;
                let mut delta = (cvt_distance - value).abs();
                if delta < cutin {
                    cvt_distance = if cvt_distance >= 0 { value } else { -value };
                }
                if self.graphics.zp1 == ZonePointer::Twilight {
                    let fv = self.graphics.freedom_vector;
                    let p = {
                        let p2 = self.graphics.zp0().original(self.graphics.rp0)?;
                        let p1 = self.graphics.zp1_mut().original_mut(point_ix)?;
                        p1.x = p2.x + mul(cvt_distance, fv.x);
                        p1.y = p2.y + mul(cvt_distance, fv.y);
                        *p1
                    };
                    *self.graphics.zp1_mut().point_mut(point_ix)? = p;
                }
                let original_distance = self.graphics.dual_project(
                    self.graphics.zp1().original(point_ix)?,
                    self.graphics.zp0().original(self.graphics.rp0)?,
                );
                let current_distance = self.graphics.project(
                    self.graphics.zp1().point(point_ix)?,
                    self.graphics.zp0().point(self.graphics.rp0)?,
                );
                if self.graphics.auto_flip && (original_distance ^ cvt_distance) < 0 {
                    cvt_distance = -cvt_distance;
                }
                let mut distance = if (opcode & 4) != 0 {
                    if self.graphics.zp0 == self.graphics.zp1 {
                        delta = (cvt_distance - original_distance).abs();
                        if delta > self.graphics.control_value_cutin {
                            cvt_distance = original_distance;
                        }
                    }
                    self.graphics.round(cvt_distance)
                } else {
                    cvt_distance
                };
                let min_distance = self.graphics.min_distance;
                if (opcode & 8) != 0 {
                    if original_distance >= 0 {
                        if distance < min_distance {
                            distance = min_distance
                        };
                    } else if distance > -min_distance {
                        distance = -min_distance
                    }
                }
                self.move_point(
                    self.graphics.zp1,
                    point_ix,
                    distance.wrapping_sub(current_distance),
                )?;
                self.graphics.rp1 = self.graphics.rp0;
                if (opcode & 16) != 0 {
                    self.graphics.rp0 = point_ix;
                }
                self.graphics.rp2 = point_ix;
            }
            _ => {
                let axis_count = self.axis_count as usize;
                if axis_count != 0 && opcode == op::GETVAR {
                    for coord in self
                        .coords
                        .iter()
                        .copied()
                        .chain(core::iter::repeat(Default::default()))
                        .take(axis_count)
                    {
                        self.value_stack.push(coord.to_bits() as i32)?;
                    }
                } else if axis_count != 0 && opcode == 0x92 {
                    self.value_stack.push(17)?;
                } else {
                    let mut index = !0;
                    for i in 0..self.instruction_defs.len() {
                        let idef = self.instruction_defs.get(i)?;
                        if idef.is_active() && idef.opcode() == Some(opcode) {
                            index = i;
                            break;
                        }
                    }
                    if index != !0 {
                        let def = self.instruction_defs.get(index)?;
                        let rec = CallRecord {
                            caller_program: program,
                            return_pc: ins.pc + 1,
                            current_count: 1,
                            definition: def,
                        };
                        self.call_stack.push(rec)?;
                        *decoder = Decoder::new(
                            def.program(),
                            programs[def.program() as usize],
                            def.range().start,
                        );
                    } else {
                        return Err(HintErrorKind::InvalidOpcode(opcode));
                    }
                }
            }
        }
        Ok(())
    }
}
