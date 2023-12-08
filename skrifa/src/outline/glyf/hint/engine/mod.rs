//! TrueType interpreter engine.

mod arith;
mod control_flow;
mod cvt;
mod dispatch;
mod logical;
mod stack;
mod storage;

use crate::{prelude::NormalizedCoord, scale::Hinting};

use super::call_stack::{CallRecord, CallStack};
use super::code::{opcodes as op, CodeDefinition, Decoder, Instruction, Program};
use super::error::HintErrorKind;
use super::graphics::{CoordAxis, GraphicsState, RetainedGraphicsState, RoundMode};
use super::value_stack::ValueStack;
use super::InstanceState;
use super::{math::*, HintError};
use super::{Zone, ZoneData};

pub type Point = super::Point<i32>;
pub type OpResult = Result<(), HintErrorKind>;
//pub type OpResult = Option<()>;

pub const TRACE: bool = false;

pub enum MaybeMut<'a, T> {
    Ref(&'a [T]),
    Mut(&'a mut [T]),
}

impl<'a, T> MaybeMut<'a, T>
where
    T: Sized + Copy + Default,
{
    pub fn len(&self) -> usize {
        match self {
            Self::Ref(defs) => defs.len(),
            Self::Mut(defs) => defs.len(),
        }
    }

    pub fn get(&self, index: usize) -> Result<T, HintErrorKind> {
        match self {
            Self::Ref(defs) => defs.get(index).copied(),
            Self::Mut(defs) => defs.get(index).copied(),
        }
        .ok_or(HintErrorKind::InvalidDefintionIndex(index))
    }

    pub fn set(&mut self, index: usize, value: T) -> Result<(), HintErrorKind> {
        match self {
            Self::Mut(defs) => {
                *defs
                    .get_mut(index)
                    .ok_or(HintErrorKind::InvalidDefintionIndex(index))? = value
            }
            _ => return Err(HintErrorKind::DefinitionInGlyphProgram),
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        match self {
            Self::Mut(defs) => defs.fill(Default::default()),
            _ => {}
        }
    }
}

/// Copy-on-write buffers for CVT and storage.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/merge_requests/23>
pub struct CvtOrStorage<'a> {
    /// True if we've initialized the mutable slice
    has_mut: bool,
    data: &'a [i32],
    data_mut: &'a mut [i32],
}

impl<'a> CvtOrStorage<'a> {
    pub fn new(data: &'a [i32], data_mut: &'a mut [i32]) -> Self {
        assert_eq!(data.len(), data_mut.len());
        Self {
            has_mut: false,
            data,
            data_mut,
        }
    }

    pub fn new_mut(data_mut: &'a mut [i32]) -> Self {
        Self {
            has_mut: true,
            data: &[],
            data_mut,
        }
    }

    pub fn get(&self, index: usize) -> Result<i32, HintErrorKind> {
        if self.has_mut {
            self.data_mut.get(index).copied()
        } else {
            self.data.get(index).copied()
        }
        .ok_or(HintErrorKind::InvalidCvtIndex(index))
    }

    pub fn set(&mut self, index: usize, value: i32) -> Result<(), HintErrorKind> {
        // Copy from immutable to mutable buffer if we haven't already
        if !self.has_mut {
            self.data_mut.copy_from_slice(self.data);
            self.has_mut = true;
        }
        *self
            .data_mut
            .get_mut(index)
            .ok_or(HintErrorKind::InvalidCvtIndex(index))? = value;
        Ok(())
    }
}

/// TrueType hinting engine.
pub struct Engine<'a> {
    value_stack: ValueStack<'a>,
    storage: CvtOrStorage<'a>,
    cvt: CvtOrStorage<'a>,
    fdefs: MaybeMut<'a, CodeDefinition>,
    idefs: MaybeMut<'a, CodeDefinition>,
    instance: InstanceState,
    graphics: GraphicsState<'a>,
    coords: &'a [NormalizedCoord],
    axis_count: u16,
    y_scale: i32,
    is_composite: bool,
    is_rotated: bool,
    call_stack: CallStack,
    did_iup_x: bool,
    did_iup_y: bool,
    is_v35: bool,
    is_subpixel: bool,
    is_grayscale: bool,
    is_grayscale_cleartype: bool,
    backward_compat_enabled: bool,
    is_pedantic: bool,
}

impl<'a> Engine<'a> {
    /// Creates a new hinting engine.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value_stack: ValueStack<'a>,
        storage: CvtOrStorage<'a>,
        cvt: CvtOrStorage<'a>,
        function_defs: MaybeMut<'a, CodeDefinition>,
        instruction_defs: MaybeMut<'a, CodeDefinition>,
        twilight: ZoneData<'a>,
        glyph: ZoneData<'a>,
        coords: &'a [NormalizedCoord],
        axis_count: u16,
    ) -> Self {
        let mut graphics = GraphicsState::default();
        graphics.zone_data = [twilight, glyph];
        Self {
            value_stack,
            call_stack: CallStack::default(),
            storage,
            cvt,
            fdefs: function_defs,
            idefs: instruction_defs,
            coords,
            axis_count,
            instance: InstanceState::default(),
            graphics,
            y_scale: 0,
            is_composite: false,
            is_rotated: false,
            did_iup_x: false,
            did_iup_y: false,
            is_v35: false,
            is_subpixel: true,
            is_grayscale: true,
            is_grayscale_cleartype: true,
            backward_compat_enabled: false,
            is_pedantic: false,
        }
    }

    pub fn run_fpgm<'b>(&mut self, state: &'b mut InstanceState, fpgm: &'a [u8]) -> bool {
        let programs = [fpgm, &[], &[]];
        self.fdefs.reset();
        self.idefs.reset();
        state.ppem = 0;
        state.scale = 0;
        state.mode = Hinting::VerticalSubpixel;
        state.graphics = RetainedGraphicsState::default();
        self.graphics = GraphicsState::default();
        let res = self.execute_all(programs, Program::Font, false);
        if !res.is_ok() {
            println!("{res:?}");
        }
        res.is_ok()
    }

    pub fn run_prep<'b>(
        &mut self,
        state: &'b mut InstanceState,
        mode: Hinting,
        fpgm: &'a [u8],
        prep: &'a [u8],
        ppem: u16,
        scale: i32,
    ) -> bool {
        let programs = [fpgm, prep, &[]];
        self.graphics.zone_mut(Zone::Twilight).clear();
        state.mode = mode;
        state.ppem = ppem;
        state.scale = scale;
        self.y_scale = state.scale;
        self.graphics = GraphicsState::default();
        self.instance = *state;
        let res = self.execute_all(programs, Program::ControlValue, false);
        if res.is_ok() {
            state.compat = self.instance.compat;
            state.graphics = self.graphics.retained;
            true
        } else {
            println!("{res:?}");
            false
        }
    }

    pub fn run<'b>(
        &mut self,
        state: &'b mut InstanceState,
        fpgm: &'a [u8],
        prep: &'a [u8],
        ins: &'a [u8],
        is_composite: bool,
    ) -> bool {
        let programs = [fpgm, prep, ins];
        self.y_scale = state.scale;
        if is_composite {
            self.y_scale = 1 << 16;
        }
        //self.graphics = GraphicsState::default();
        if state.graphics.instruct_control & 2 == 0 {
            self.graphics.retained = state.graphics;
        } else {
            self.graphics.retained = Default::default();
        }
        self.instance = *state;
        let res = self.execute_all(programs, Program::Glyph, is_composite);
        self.y_scale = state.scale;
        if res.is_ok() {
            state.compat = self.instance.compat;
            true
        } else {
            println!("{res:?}");
            false
        }
    }
}

impl<'a> Engine<'a> {
    fn move_original(
        &mut self,
        zone: Zone,
        point_ix: usize,
        distance: i32,
    ) -> Result<(), HintErrorKind> {
        let fdotp = self.graphics.fdotp;
        let fv = self.graphics.freedom_vector;
        let fv_axes = self.graphics.freedom_axes;
        let point = self.graphics.zone_mut(zone).original_mut(point_ix)?;
        match fv_axes {
            CoordAxis::X => point.x += distance,
            CoordAxis::Y => point.y += distance,
            CoordAxis::Both => {
                if fv.x != 0 {
                    point.x += muldiv(distance, fv.x, fdotp);
                }
                if fv.y != 0 {
                    point.y += muldiv(distance, fv.y, fdotp);
                }
            }
        }
        Ok(())
    }

    fn move_point(
        &mut self,
        zone: Zone,
        point_ix: usize,
        distance: i32,
    ) -> Result<(), HintErrorKind> {
        let legacy = self.is_v35;
        let bc = self.backward_compat_enabled;
        let iupx = self.did_iup_x;
        let iupy = self.did_iup_y;
        let fdotp = self.graphics.fdotp;
        let fv = self.graphics.freedom_vector;
        let fv_axes = self.graphics.freedom_axes;
        let zone = self.graphics.zone_mut(zone);
        let point = zone.point_mut(point_ix)?;
        match fv_axes {
            CoordAxis::X => {
                if legacy || !bc {
                    point.x += distance;
                }
                zone.touch(point_ix, CoordAxis::X)?;
            }
            CoordAxis::Y => {
                if !(!legacy && bc && iupx && iupy) {
                    point.y += distance;
                }
                zone.touch(point_ix, CoordAxis::Y)?;
            }
            CoordAxis::Both => {
                if fv.x != 0 {
                    if legacy || !bc {
                        point.x += muldiv(distance, fv.x, fdotp);
                    }
                    zone.touch(point_ix, CoordAxis::X)?;
                }
                if fv.y != 0 {
                    if !(!legacy && bc && iupx && iupy) {
                        zone.point_mut(point_ix)?.y += muldiv(distance, fv.y, fdotp);
                    }
                    zone.touch(point_ix, CoordAxis::Y)?;
                }
            }
        }
        Ok(())
    }

    fn move_zp2_point(
        &mut self,
        point_ix: usize,
        dx: i32,
        dy: i32,
        do_touch: bool,
    ) -> Result<(), HintErrorKind> {
        let is_v35 = self.is_v35;
        let fv = self.graphics.freedom_vector;
        let (iupx, iupy) = (self.did_iup_x, self.did_iup_y);
        let compat = self.backward_compat_enabled;
        let zone = self.graphics.zp2_mut();
        if fv.x != 0 {
            if is_v35 || !compat {
                zone.point_mut(point_ix)?.x += dx;
            }
            if do_touch {
                zone.touch(point_ix, CoordAxis::X)?;
            }
        }
        if fv.y != 0 {
            if !(!is_v35 && compat && iupx && iupy) {
                zone.point_mut(point_ix)?.y += dy;
            }
            if do_touch {
                zone.touch(point_ix, CoordAxis::Y)?;
            }
        }
        Ok(())
    }

    fn point_displacement(
        &mut self,
        opcode: u8,
        rp1: usize,
        rp2: usize,
    ) -> Result<PointDisplacement, HintErrorKind> {
        let (zone, point_ix) = if (opcode & 1) != 0 {
            (self.graphics.zp0, rp1)
        } else {
            (self.graphics.zp1, rp2)
        };
        let zone_data = self.graphics.zone(zone);
        let point = zone_data.point(point_ix)?;
        let original_point = zone_data.original(point_ix)?;
        let distance = self.graphics.project(point, original_point);
        let fv = self.graphics.freedom_vector;
        let fdotp = self.graphics.fdotp;
        let dx = muldiv(distance, fv.x, fdotp);
        let dy = muldiv(distance, fv.y, fdotp);
        Ok(PointDisplacement {
            zone,
            point_ix,
            dx,
            dy,
        })
    }
}

struct PointDisplacement {
    pub zone: Zone,
    pub point_ix: usize,
    pub dx: i32,
    pub dy: i32,
}

impl<'a> Engine<'a> {
    fn execute(
        &mut self,
        programs: [&'a [u8]; 3],
        program: Program,
        is_composite: bool,
    ) -> Result<u32, HintErrorKind> {
        self.value_stack.clear();
        let mut decoder = Decoder::new(program, programs[program as usize], 0);
        if decoder.bytecode.is_empty() {
            return Ok(0);
        }
        let (v35, grayscale, subpixel, grayscale_cleartype) = match self.instance.mode {
            Hinting::None => return Ok(0),
            Hinting::Full => (true, true, false, false),
            Hinting::Light => (false, false, true, true),
            Hinting::LightSubpixel => (false, false, true, false),
            Hinting::VerticalSubpixel => (false, false, true, false),
        };
        self.is_v35 = v35;
        self.is_subpixel = subpixel;
        if self.instance.mode == Hinting::VerticalSubpixel {
            self.backward_compat_enabled = true;
        } else if !v35 && subpixel {
            self.backward_compat_enabled = (self.graphics.instruct_control & 0x4) == 0;
        } else {
            self.backward_compat_enabled = false;
        }
        self.is_composite = is_composite;
        self.is_grayscale = grayscale;
        self.is_grayscale_cleartype = grayscale_cleartype;
        // self.backward_compat_enabled = true;
        self.instance.compat = self.backward_compat_enabled;
        self.graphics.update_projection_state();
        self.graphics.reset_zone_pointers();
        self.graphics.rp0 = 0;
        self.graphics.rp1 = 0;
        self.graphics.rp1 = 0;
        self.did_iup_x = false;
        self.did_iup_y = false;
        self.graphics.loop_counter = 1;
        let mut count = 0u32;
        loop {
            let Some(decoded) = decoder.maybe_next() else {
                if self.call_stack.len() > 0 {
                    return Err(HintErrorKind::CallStackUnderflow);
                }
                break;
            };
            let ins = decoded?;

            if TRACE {
                let name = ins.name();
                for _ in 0..self.call_stack.len() {
                    print!(".");
                }
                print!("{} [{}] {}", count, ins.pc, name);
                let pcnt = if self.value_stack.len() < 16 {
                    self.value_stack.len()
                } else {
                    16
                };
                for i in 1..=pcnt {
                    print!(" {}", self.value_stack.values()[self.value_stack.len() - i]);
                }
                println!();
            }

            let opcode = ins.opcode;
            match opcode {
                op::SVTCA0..=op::SFVTCA1 => {
                    let aa = ((opcode as i32 & 1) << 14);
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
                    let discriminant = muldiv(dax, -dby, 0x40) + muldiv(day, dbx, 0x40);
                    let dp = muldiv(dax, dbx, 0x40) + muldiv(day, dby, 0x40);
                    if 19 * discriminant.abs() > dp.abs() {
                        let v = muldiv(dx, -dby, 0x40) + muldiv(dy, dbx, 0x40);
                        let x = muldiv(v, dax, discriminant);
                        let y = muldiv(v, day, discriminant);
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
                    self.graphics.zp0 = Zone::try_from(z)?;
                }
                op::SZP1 => {
                    let z = self.value_stack.pop()?;
                    self.graphics.zp1 = Zone::try_from(z)?;
                }
                op::SZP2 => {
                    let z = self.value_stack.pop()?;
                    self.graphics.zp2 = Zone::try_from(z)?;
                }
                op::SZPS => {
                    let z = self.value_stack.pop()?;
                    let zp = Zone::try_from(z)?;
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
                op::ELSE => self.op_else(&mut decoder)?,
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
                        let def = self.fdefs.get(def_ix)?;
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
                        decoder = Decoder::new(
                            def.program(),
                            programs[def.program() as usize],
                            def.range().start,
                        );
                    }
                }
                op::FDEF => {
                    let def_ix = self.value_stack.pop_usize()?;
                    if program == Program::Glyph || def_ix >= self.fdefs.len() {
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
                                self.fdefs.set(def_ix, def)?;
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
                        decoder = Decoder::new(
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
                    let is_x = (opcode & 1) != 0;
                    let mut run = true;
                    if !self.is_v35 && self.backward_compat_enabled {
                        if self.did_iup_x && self.did_iup_y {
                            run = false;
                        }
                        if is_x {
                            self.did_iup_x = true;
                        } else {
                            self.did_iup_y = true;
                        }
                    }
                    if run {
                        self.graphics.zone_mut(Zone::Glyph).iup(is_x)?;
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
                    let bound = if self.graphics.zp2 == Zone::Twilight {
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
                    let limit = if self.graphics.zp2 == Zone::Twilight {
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
                    let limit = if self.graphics.zp2 == Zone::Twilight {
                        self.graphics.zp2().points.len()
                    } else if self.graphics.zp2 == Zone::Glyph
                        && !self.graphics.zp2().contours.is_empty()
                    {
                        let z = self.graphics.zp2();
                        *z.contours
                            .last()
                            .ok_or(HintErrorKind::InvalidContourIndex(0))?
                            as usize
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
                    let in_twilight = self.graphics.zp0 == Zone::Twilight
                        || self.graphics.zp1 == Zone::Twilight
                        || self.graphics.zp2 == Zone::Twilight;
                    let a = self.value_stack.pop()?;
                    let dx = mul14(a, self.graphics.freedom_vector.x as i32);
                    let dy = mul14(a, self.graphics.freedom_vector.y as i32);
                    let mut iters = core::mem::replace(&mut self.graphics.loop_counter, 1);
                    while iters > 0 {
                        let point = self.value_stack.pop_usize()?;
                        if !self.is_v35 && self.backward_compat_enabled {
                            if in_twilight
                                || (!(self.did_iup_x && self.did_iup_y)
                                    && ((is_composite && self.graphics.freedom_vector.y != 0)
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
                    let in_twilight = self.graphics.zp0 == Zone::Twilight
                        || self.graphics.zp1 == Zone::Twilight
                        || self.graphics.zp2 == Zone::Twilight;
                    let orus_base = if in_twilight {
                        self.graphics.zp0().original(self.graphics.rp1)?
                    } else {
                        self.graphics.zp0().unscaled(self.graphics.rp1)?
                    };
                    let cur_base = self.graphics.zp0().point(self.graphics.rp1)?;
                    let old_range = if in_twilight {
                        self.graphics.dual_project(
                            self.graphics.zp1().original(self.graphics.rp2)?,
                            orus_base,
                        )
                    } else {
                        self.graphics.dual_project(
                            self.graphics.zp1().unscaled(self.graphics.rp2)?,
                            orus_base,
                        )
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
                                new_distance = muldiv(original_distance, cur_range, old_range);
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
                    if self.graphics.zp1 == Zone::Twilight {
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
                    if self.graphics.zp0 == Zone::Twilight {
                        let fv = self.graphics.freedom_vector;
                        let z = self.graphics.zp0_mut();
                        let original_point = z.original_mut(point_ix)?;
                        original_point.x = mul14(distance, fv.x as i32);
                        original_point.y = mul14(distance, fv.y as i32);
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
                    if self.graphics.zp2 == Zone::Twilight {
                        let twilight = self.graphics.zone_mut(Zone::Twilight);
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
                    } else if self.graphics.zp0 == Zone::Twilight
                        || self.graphics.zp1 == Zone::Twilight
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
                        muldiv(self.instance.ppem as i32, 64 * 72, 72)
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
                op::IF => self.op_if(&mut decoder)?,
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
                                    && ((is_composite && self.graphics.freedom_vector.y != 0)
                                        || self
                                            .graphics
                                            .zp0()
                                            .is_touched(point_ix, CoordAxis::Y)?)
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
                op::JROT => self.op_jrot(&mut decoder)?,
                op::JMPR => self.op_jmpr(&mut decoder)?,
                op::JROF => self.op_jrof(&mut decoder)?,
                op::ROFF => self.graphics.round_state.mode = RoundMode::Off,
                op::RUTG => self.graphics.round_state.mode = RoundMode::UpToGrid,
                op::RDTG => self.graphics.round_state.mode = RoundMode::DownToGrid,
                op::SANGW => {}
                op::AA => {}
                op::FLIPPT => {
                    if !self.is_v35
                        && self.backward_compat_enabled
                        && self.did_iup_x
                        && self.did_iup_y
                    {
                        // nothing
                    } else {
                        let mut iters = core::mem::replace(&mut self.graphics.loop_counter, 1);
                        while iters > 0 {
                            let point = self.value_stack.pop_usize()?;
                            self.graphics.zone_mut(Zone::Glyph).flip_on_curve(point)?;
                            iters -= 1;
                        }
                    }
                }
                op::FLIPRGON | op::FLIPRGOFF => {
                    if !self.is_v35
                        && self.backward_compat_enabled
                        && self.did_iup_x
                        && self.did_iup_y
                    {
                        // nothing
                    } else {
                        let last_point_ix = self.value_stack.pop_usize()?;
                        let first_point_ix = self.value_stack.pop_usize()?;
                        if first_point_ix > last_point_ix {
                            return Err(HintErrorKind::InvalidPointIndex(first_point_ix));
                        }
                        self.graphics.zone_mut(Zone::Glyph).set_on_curve(
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
                    let a = self.value_stack.pop()?;
                    let mut k = 0;
                    if (a & 1) != 0 {
                        k = if self.is_v35 { 35 } else { 42 };
                    }
                    if (a & 2) != 0 && self.is_rotated {
                        k |= 1 << 8;
                    }
                    if (a & 8) != 0 && !self.coords.is_empty() {
                        k |= 1 << 10;
                    }
                    if (a & 32) != 0 && grayscale {
                        k |= 1 << 12;
                    }
                    if !self.is_v35 && self.is_subpixel {
                        if (a & 64) != 0 {
                            k |= 1 << 13;
                        }
                        // if (a & 256) != 0 && false
                        // /* self.vertical_lcd */
                        // {
                        //     k |= 1 << 15;
                        // }
                        if (a & 1024) != 0 {
                            k |= 1 << 17;
                        }
                        // remove me
                        if (a & 2048) != 0 && self.is_subpixel {
                            k |= 1 << 18;
                        }

                        if (a & 4096) != 0 && grayscale_cleartype {
                            k |= 1 << 19;
                        }
                    }
                    self.value_stack.push(k)?;
                }
                op::IDEF => {
                    if program == Program::Glyph {
                        return Err(HintErrorKind::DefinitionInGlyphProgram);
                    }
                    let def_ix = self.value_stack.pop_usize()?;
                    let mut index = !0;
                    for i in 0..self.idefs.len() {
                        if !self.idefs.get(i)?.is_active() {
                            index = i;
                            break;
                        }
                    }
                    if index == !0 {
                        return Err(HintErrorKind::InvalidDefintionIndex(self.idefs.len()));
                    }
                    let start = ins.pc + 1;
                    while let Some(next_ins) = decoder.maybe_next() {
                        let next_ins = next_ins?;
                        match next_ins.opcode {
                            op::IDEF | op::FDEF => {
                                return Err(HintErrorKind::NestedDefinition);
                            }
                            op::ENDF => {
                                let def = CodeDefinition::new(
                                    program,
                                    start..decoder.pc,
                                    Some(def_ix as u8),
                                );
                                self.idefs.set(index, def)?;
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
                        if selector == 3
                            && !self.is_v35
                            && self.instance.mode != Hinting::VerticalSubpixel
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
                    if self.graphics.zp0 == Zone::Twilight || self.graphics.zp1 == Zone::Twilight {
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
                    if self.graphics.zp1 == Zone::Twilight {
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
                        for i in 0..self.idefs.len() {
                            let idef = self.idefs.get(i)?;
                            if idef.is_active() && idef.opcode() == Some(opcode) {
                                index = i;
                                break;
                            }
                        }
                        if index != !0 {
                            let def = self.idefs.get(index)?;
                            let rec = CallRecord {
                                caller_program: program,
                                return_pc: ins.pc + 1,
                                current_count: 1,
                                definition: def,
                            };
                            self.call_stack.push(rec)?;
                            decoder = Decoder::new(
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

            if TRACE {
                // if trpt < self.glyph.points.len() {
                //     println!(
                //         ">>>>>> {}, {}",
                //         self.glyph.points[trpt].x, self.glyph.points[trpt].y
                //     );
                // }
            }

            count += 1;
            if decoder.pc >= decoder.bytecode.len() {
                if !self.call_stack.is_empty() {
                    return Err(HintErrorKind::CallStackUnderflow);
                }
                break;
            }
        }
        Ok(count)
    }

    fn execute_all(
        &mut self,
        programs: [&'a [u8]; 3],
        program: Program,
        is_composite: bool,
    ) -> Result<u32, HintError> {
        self.value_stack.clear();
        let mut decoder = Decoder::new(program, programs[program as usize], 0);
        if decoder.bytecode.is_empty() {
            return Ok(0);
        }
        let (v35, grayscale, subpixel, grayscale_cleartype) = match self.instance.mode {
            Hinting::None => return Ok(0),
            Hinting::Full => (true, true, false, false),
            Hinting::Light => (false, false, true, true),
            Hinting::LightSubpixel => (false, false, true, false),
            Hinting::VerticalSubpixel => (false, false, true, false),
        };
        self.is_v35 = v35;
        self.is_subpixel = subpixel;
        if self.instance.mode == Hinting::VerticalSubpixel {
            self.backward_compat_enabled = true;
        } else if !v35 && subpixel {
            self.backward_compat_enabled = (self.graphics.instruct_control & 0x4) == 0;
        } else {
            self.backward_compat_enabled = false;
        }
        self.is_composite = is_composite;
        self.is_grayscale = grayscale;
        self.is_grayscale_cleartype = grayscale_cleartype;
        // self.backward_compat_enabled = true;
        self.instance.compat = self.backward_compat_enabled;
        self.graphics.update_projection_state();
        self.graphics.reset_zone_pointers();
        self.graphics.rp0 = 0;
        self.graphics.rp1 = 0;
        self.graphics.rp1 = 0;
        self.did_iup_x = false;
        self.did_iup_y = false;
        self.graphics.loop_counter = 1;
        let mut count = 0u32;
        loop {
            let Some(decoded) = decoder.maybe_next() else {
                if !self.call_stack.is_empty() {
                    return Err(HintError::new(&decoder, HintErrorKind::CallStackUnderflow));
                }
                break;
            };
            let cur_program = decoder.program;
            let cur_pc = decoder.pc;

            let ins = match decoded {
                Ok(ins) => ins,
                Err(kind) => {
                    return Err(HintError {
                        program: cur_program,
                        pc: cur_pc,
                        kind: kind.into(),
                    })
                }
            };

            if TRACE {
                let name = ins.name();
                for _ in 0..self.call_stack.len() {
                    print!(".");
                }
                print!("{} [{}] {}", count, ins.pc, name);
                let pcnt = if self.value_stack.len() < 16 {
                    self.value_stack.len()
                } else {
                    16
                };
                for i in 1..=pcnt {
                    print!(" {}", self.value_stack.values()[self.value_stack.len() - i]);
                }
                println!();
            }

            if let Err(kind) = self.dispatch(&programs, program, &mut decoder, &ins) {
                return Err(HintError {
                    program: cur_program,
                    pc: cur_pc,
                    kind,
                });
            }
            if TRACE {
                // if trpt < self.glyph.points.len() {
                //     println!(
                //         ">>>>>> {}, {}",
                //         self.glyph.points[trpt].x, self.glyph.points[trpt].y
                //     );
                // }
            }
            count += 1;
        }
        Ok(count)
    }
}

impl HintError {
    pub(crate) fn new(decoder: &Decoder, kind: HintErrorKind) -> Self {
        Self {
            program: decoder.program,
            pc: decoder.pc,
            kind,
        }
    }
}
