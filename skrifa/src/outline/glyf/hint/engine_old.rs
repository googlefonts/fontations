use crate::{prelude::NormalizedCoord, scale::Hinting};
use read_fonts::tables::glyf::PointMarker;

use super::code::{opcodes as op, DecodeError, Decoder, Definition, Program};
use super::math::*;
use super::state::*;
use super::zone::{Zone, ZoneData, ZoneState};

pub type Point = super::Point<i32>;

pub const TRACE: bool = false;

#[derive(Clone, Debug)]
pub enum HintError {
    Decode(DecodeError),
    InvalidStackReference,
    InvalidPointReference,
}

impl From<DecodeError> for HintError {
    fn from(value: DecodeError) -> Self {
        Self::Decode(value)
    }
}

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

    pub fn get(&self, index: usize) -> Option<T> {
        match self {
            Self::Ref(defs) => defs.get(index).copied(),
            Self::Mut(defs) => defs.get(index).copied(),
        }
    }

    pub fn set(&mut self, index: usize, value: T) -> Option<()> {
        match self {
            Self::Mut(defs) => *defs.get_mut(index)? = value,
            _ => return None,
        }
        Some(())
    }

    pub fn reset(&mut self) {
        match self {
            Self::Mut(defs) => defs.fill(Default::default()),
            _ => {}
        }
    }
}

/// TrueType hinting engine.
pub struct Engine<'a> {
    store: MaybeMut<'a, i32>,
    cvt: MaybeMut<'a, i32>,
    fdefs: MaybeMut<'a, Definition>,
    idefs: MaybeMut<'a, Definition>,
    zone: ZoneState<'a>,
    coords: &'a [NormalizedCoord],
    axis_count: u16,
    ppem: u16,
    point_size: i32,
    scale: i32,
    yscale: i32,
    rotated: bool,
    round: RoundState,
    project: ProjectState,
    iupx: bool,
    iupy: bool,
    v35: bool,
    subpixel: bool,
    compat: bool,
}

impl<'a> Engine<'a> {
    /// Creates a new hinting engine.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        storage: MaybeMut<'a, i32>,
        cvt: MaybeMut<'a, i32>,
        function_defs: MaybeMut<'a, Definition>,
        instruction_defs: MaybeMut<'a, Definition>,
        twilight: ZoneData<'a>,
        glyph: ZoneData<'a>,
        coords: &'a [NormalizedCoord],
        axis_count: u16,
    ) -> Self {
        Self {
            store: storage,
            cvt,
            fdefs: function_defs,
            idefs: instruction_defs,
            zone: ZoneState::new(glyph, twilight),
            coords,
            axis_count,
            ppem: 0,
            point_size: 0,
            scale: 0,
            yscale: 0,
            rotated: false,
            iupx: false,
            iupy: false,
            round: RoundState::default(),
            project: ProjectState::default(),
            v35: false,
            subpixel: true,
            compat: false,
        }
    }

    pub fn run_fpgm<'b>(
        &mut self,
        state: &'b mut InstanceState,
        stack: Stack<'b>,
        fpgm: &[u8],
    ) -> bool {
        let programs = [fpgm, &[], &[]];
        self.fdefs.reset();
        self.idefs.reset();
        state.ppem = 0;
        state.point_size = 0;
        state.scale = 0;
        state.mode = Hinting::VerticalSubpixel;
        state.graphics = GraphicsState::default();
        state.default_graphics = GraphicsState::default();
        state.coord_count = self.axis_count;
        self.execute(state, stack, programs, Program::Font, false)
            .is_some()
    }

    pub fn run_prep<'b>(
        &mut self,
        state: &'b mut InstanceState,
        mode: Hinting,
        stack: Stack<'b>,
        fpgm: &[u8],
        prep: &[u8],
        ppem: u16,
        scale: i32,
    ) -> bool {
        let programs = [fpgm, prep, &[]];
        self.zone.twilight.clear();
        state.mode = mode;
        state.ppem = ppem;
        state.scale = scale;
        state.point_size = muldiv(ppem as i32, 64 * 72, 72);
        state.coord_count = self.axis_count;
        self.ppem = state.ppem;
        self.point_size = state.point_size;
        self.scale = state.scale;
        self.yscale = state.scale;
        let res = self.execute(state, stack, programs, Program::Preprogram, false);
        if res.is_some() {
            state.default_graphics = state.graphics;
            true
        } else {
            false
        }
    }

    pub fn run<'b>(
        &mut self,
        state: &'b mut InstanceState,
        stack: Stack<'b>,
        fpgm: &[u8],
        prep: &[u8],
        ins: &[u8],
        is_composite: bool,
    ) -> bool {
        let programs = [fpgm, prep, ins];
        self.ppem = state.ppem;
        self.point_size = state.point_size;
        self.scale = state.scale;
        self.yscale = state.scale;
        if is_composite {
            self.yscale = 1 << 16;
        }
        if state.default_graphics.instruct_control & 2 != 0 {
            state.graphics = GraphicsState::default();
        } else {
            state.graphics = state.default_graphics;
        }
        let res = self.execute(state, stack, programs, Program::Glyph, is_composite);
        self.yscale = self.scale;
        res.is_some()
    }
}

impl<'a> Engine<'a> {
    fn round(&self, distance: i32) -> i32 {
        self.round.round(distance)
    }

    fn move_original(&mut self, zone: Zone, point_ix: usize, distance: i32) -> Option<()> {
        let fdotp = self.project.fdotp;
        let x = self.project.fv.x;
        let y = self.project.fv.y;
        let state = self.project.move_mode;
        let p = self.zone.get_mut(zone).original_mut(point_ix)?;
        match state {
            CoordMode::X => p.x += distance,
            CoordMode::Y => p.y += distance,
            CoordMode::Both => {
                if x != 0 {
                    p.x += muldiv(distance, x as i32, fdotp);
                }
                if y != 0 {
                    p.y += muldiv(distance, y as i32, fdotp);
                }
            }
        }
        Some(())
    }

    fn move_point(&mut self, zone: Zone, point_ix: usize, distance: i32) -> Option<()> {
        let legacy = self.v35;
        let bc = self.compat;
        let iupx = self.iupx;
        let iupy = self.iupy;
        let x = self.project.fv.x;
        let y = self.project.fv.y;
        let fdotp = self.project.fdotp;
        let state = self.project.move_mode;
        let z = self.zone.get_mut(zone);
        let p = z.point_mut(point_ix)?;
        match state {
            CoordMode::X => {
                if legacy || !bc {
                    p.x += distance;
                }
                z.flags_mut(point_ix)?.set_marker(PointMarker::TOUCHED_X);
            }
            CoordMode::Y => {
                if !(!legacy && bc && iupx && iupy) {
                    p.y += distance;
                }
                z.flags_mut(point_ix)?.set_marker(PointMarker::TOUCHED_Y);
            }
            CoordMode::Both => {
                if x != 0 {
                    if legacy || !bc {
                        p.x += muldiv(distance, x as i32, fdotp);
                    }
                    z.flags_mut(point_ix)?.set_marker(PointMarker::TOUCHED_X);
                }
                if y != 0 {
                    if !(!legacy && bc && iupx && iupy) {
                        z.point_mut(point_ix)?.y += muldiv(distance, y as i32, fdotp);
                    }
                    z.flags_mut(point_ix)?.set_marker(PointMarker::TOUCHED_Y);
                }
            }
        }
        Some(())
    }

    #[inline(always)]
    fn project(&self, v1: Point, v2: Point) -> i32 {
        self.project.project(v1, v2)
    }

    #[inline(always)]
    fn dual_project(&self, v1: Point, v2: Point) -> i32 {
        self.project.dual_project(v1, v2)
    }

    #[inline(always)]
    fn fast_project(&self, v: Point) -> i32 {
        self.project(v, Point::new(0, 0))
    }

    #[inline(always)]
    fn fast_dual_project(&self, v: Point) -> i32 {
        self.dual_project(v, Point::new(0, 0))
    }

    fn compute_point_displacement(
        &mut self,
        opcode: u8,
        rp1: usize,
        rp2: usize,
    ) -> Option<(i32, i32, Zone, usize)> {
        let (zone, point_ix) = if (opcode & 1) != 0 {
            (self.zone.zp0, rp1)
        } else {
            (self.zone.zp1, rp2)
        };
        let z = self.zone.get(zone);
        let point = z.point(point_ix)?;
        let original = z.original(point_ix)?;
        let d = self.project(point, original);
        let x = muldiv(d, self.project.fv.x as i32, self.project.fdotp);
        let y = muldiv(d, self.project.fv.y as i32, self.project.fdotp);
        Some((x, y, zone, point_ix))
    }

    fn move_zp2_point(&mut self, point_ix: usize, dx: i32, dy: i32, touch: bool) -> Option<()> {
        let v35 = self.v35;
        let fv = self.project.fv;
        let (iupx, iupy) = (self.iupx, self.iupy);
        let compat = self.compat;
        let zone = self.zone.zp2_mut();
        if fv.x != 0 {
            if v35 || !compat {
                zone.point_mut(point_ix)?.x += dx;
            }
            if touch {
                zone.flags_mut(point_ix)?.set_marker(PointMarker::TOUCHED_X);
            }
        }
        if fv.y != 0 {
            if !(!v35 && compat && iupx && iupy) {
                zone.point_mut(point_ix)?.y += dy;
            }
            if touch {
                zone.flags_mut(point_ix)?.set_marker(PointMarker::TOUCHED_Y);
            }
        }
        Some(())
    }

    fn normalize(&self, x: i32, y: i32, r: &mut Point) {
        use core::num::Wrapping;
        let (mut sx, mut sy) = (Wrapping(1i32), Wrapping(1i32));
        let mut ux = Wrapping(x as u32);
        let mut uy = Wrapping(y as u32);
        const ZERO: Wrapping<u32> = Wrapping(0);
        if x < 0 {
            ux = ZERO - ux;
            sx = -sx;
        }
        if y < 0 {
            uy = ZERO - uy;
            sy = -sy;
        }
        if ux == ZERO {
            r.x = x / 4;
            if uy.0 > 0 {
                r.y = (sy * Wrapping(0x10000) / Wrapping(4)).0;
            }
            return;
        }
        if uy == ZERO {
            r.y = y / 4;
            if ux.0 > 0 {
                r.x = (sx * Wrapping(0x10000) / Wrapping(4)).0;
            }
            return;
        }
        let mut len = if ux > uy {
            ux + (uy >> 1)
        } else {
            uy + (ux >> 1)
        };
        let mut shift = Wrapping(len.0.leading_zeros() as i32);
        shift -= Wrapping(15)
            + if len >= (Wrapping(0xAAAAAAAAu32) >> shift.0 as usize) {
                Wrapping(1)
            } else {
                Wrapping(0)
            };
        if shift.0 > 0 {
            let s = shift.0 as usize;
            ux <<= s;
            uy <<= s;
            len = if ux > uy {
                ux + (uy >> 1)
            } else {
                uy + (ux >> 1)
            };
        } else {
            let s = -shift.0 as usize;
            ux >>= s;
            uy >>= s;
            len >>= s;
        }
        let mut b = Wrapping(0x10000) - Wrapping(len.0 as i32);
        let x = Wrapping(ux.0 as i32);
        let y = Wrapping(uy.0 as i32);
        let mut z;
        let mut u;
        let mut v;
        loop {
            u = Wrapping((x + ((x * b) >> 16)).0 as u32);
            v = Wrapping((y + ((y * b) >> 16)).0 as u32);
            z = Wrapping(-((u * u + v * v).0 as i32)) / Wrapping(0x200);
            z = z * ((Wrapping(0x10000) + b) >> 8) / Wrapping(0x10000);
            b += z;
            if z <= Wrapping(0) {
                break;
            }
        }
        r.x = (Wrapping(u.0 as i32) * sx / Wrapping(4)).0;
        r.y = (Wrapping(v.0 as i32) * sy / Wrapping(4)).0;
    }
}

impl<'a> Engine<'a> {
    fn execute<'b>(
        &mut self,
        state: &'b mut InstanceState,
        mut stack: Stack<'b>,
        programs: [&[u8]; 3],
        program: Program,
        composite: bool,
    ) -> Option<u32> {
        let mut decoder = Decoder::new(program, programs[program as usize], 0);
        if decoder.bytecode.is_empty() {
            return Some(0);
        }
        let (v35, grayscale, subpixel, grayscale_cleartype) = match state.mode {
            Hinting::None => return Some(0),
            Hinting::Full => (true, true, false, false),
            Hinting::Light => (false, false, true, true),
            Hinting::LightSubpixel => (false, false, true, false),
            Hinting::VerticalSubpixel => (false, false, true, false),
        };
        self.v35 = v35;
        self.subpixel = subpixel;
        if state.mode == Hinting::VerticalSubpixel {
            self.compat = true;
        } else if !v35 && subpixel {
            self.compat = (state.graphics.instruct_control & 0x4) == 0;
        } else {
            self.compat = false;
        }
        self.compat = true;
        state.compat = self.compat;
        self.project = ProjectState::default();
        self.project.update();
        self.zone.reset_pointers();
        self.round = RoundState::default();
        self.iupx = false;
        self.iupy = false;
        let mut stack_top = 0usize;
        let mut new_top: usize;
        let mut args_top: usize;
        let mut count = 0u32;
        #[derive(Copy, Clone, Default)]
        struct CallRecord {
            caller_program: Program,
            return_pc: usize,
            current_count: u32,
            definition: Definition,
        }
        let mut callstack = [CallRecord {
            caller_program: Program::Font,
            return_pc: 0,
            current_count: 0,
            definition: Default::default(),
        }; 32];
        let mut callstack_top = 0;
        let callstack_len = callstack.len();
        let stack_size = stack.values.len();
        let mut rp0 = 0usize;
        let mut rp1 = 0usize;
        let mut rp2 = 0usize;
        let mut loop_counter = 1u32;
        loop {
            let Some(decoded) = decoder.next() else {
                if callstack_top > 0 {
                    return None;
                }
                break;
            };
            let ins = decoded.ok()?;
            if ins.pop_count > stack_top {
                return None;
            }
            let args = stack_top - ins.pop_count;
            args_top = args;
            new_top = args + ins.push_count;
            if new_top > stack_size {
                return None;
            }

            if TRACE {
                let name = ins.name();
                for _ in 0..callstack_top {
                    print!(".");
                }
                print!("{} [{}] {}", count, ins.pc, name);
                let pcnt = if stack_top < 16 { stack_top } else { 16 };
                for i in 1..=pcnt {
                    print!(" {}", stack.values[stack_top - i]);
                }
                println!();
            }

            let a0 = args;
            let a1 = args + 1;
            let a2 = args + 2;

            let opcode = ins.opcode;
            match opcode {
                op::SVTCA0..=op::SFVTCA1 => {
                    let aa = ((opcode as i32 & 1) << 14) as i32;
                    let bb = aa ^ 0x4000;
                    if opcode < 4 {
                        self.project.pv = Point::new(aa, bb);
                        self.project.dv = self.project.pv;
                    }
                    if (opcode & 2) == 0 {
                        self.project.fv = Point::new(aa, bb);
                    }
                    self.project.update();
                }
                op::SPVTL0..=op::SFVTL1 => {
                    let index1 = stack.get(a1)? as usize;
                    let index2 = stack.get(a0)? as usize;
                    let mut v = Point::new(0, 0);
                    let p1 = self.zone.zp1().point(index2)?;
                    let p2 = self.zone.zp2().point(index1)?;
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
                    self.normalize(a, b, &mut v);
                    if opcode <= op::SPVTL1 {
                        self.project.pv = v;
                        self.project.dv = v;
                    } else {
                        self.project.fv = v;
                    }
                    self.project.update();
                }
                op::SPVFS => {
                    let y = stack.get(a1)? as i16 as i32;
                    let x = stack.get(a0)? as i16 as i32;
                    let mut v = self.project.pv;
                    self.normalize(x, y, &mut v);
                    self.project.pv = v;
                    self.project.dv = v;
                    self.project.update();
                }
                op::SFVFS => {
                    let y = stack.get(a1)? as i16 as i32;
                    let x = stack.get(a0)? as i16 as i32;
                    let mut v = self.project.fv;
                    self.normalize(x, y, &mut v);
                    self.project.fv = v;
                    self.project.update();
                }
                op::GPV => {
                    *stack.get_mut(a0)? = self.project.pv.x;
                    *stack.get_mut(a1)? = self.project.pv.y;
                }
                op::GFV => {
                    *stack.get_mut(a0)? = self.project.fv.x;
                    *stack.get_mut(a1)? = self.project.fv.y;
                }
                op::SFVTPV => {
                    self.project.fv = self.project.pv;
                    self.project.update();
                }
                op::ISECT => {
                    let point_ix = stack.get(args)? as usize;
                    let a0 = stack.get(args + 1)? as usize;
                    let a1 = stack.get(args + 2)? as usize;
                    let b0 = stack.get(args + 3)? as usize;
                    let b1 = stack.get(args + 4)? as usize;
                    let (pa0, pa1) = {
                        let z = self.zone.zp1();
                        (z.point(a0)?, z.point(a1)?)
                    };
                    let (pb0, pb1) = {
                        let z = self.zone.zp0();
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
                        let point = self.zone.zp2_mut().point_mut(point_ix)?;
                        point.x = pa0.x + x;
                        point.y = pa0.y + y;
                    } else {
                        let point = self.zone.zp2_mut().point_mut(point_ix)?;
                        point.x = (pa0.x + pa1.x + pb0.x + pb1.x) / 4;
                        point.y = (pa0.y + pa1.y + pb0.y + pb1.y) / 4;
                    }
                    self.zone
                        .zp2_mut()
                        .flags_mut(point_ix)?
                        .set_marker(PointMarker::TOUCHED);
                }
                op::SRP0 => rp0 = stack.get(a0)? as usize,
                op::SRP1 => rp1 = stack.get(a0)? as usize,
                op::SRP2 => rp2 = stack.get(a0)? as usize,
                op::SZP0 => {
                    let z = stack.get(a0)? as u8;
                    self.zone.zp0 = Zone::try_new(z)?;
                }
                op::SZP1 => {
                    let z = stack.get(a0)? as u8;
                    self.zone.zp1 = Zone::try_new(z)?;
                }
                op::SZP2 => {
                    let z = stack.get(a0)? as u8;
                    self.zone.zp2 = Zone::try_new(z)?;
                }
                op::SZPS => {
                    let z = stack.get(a0)? as u8;
                    let zp = Zone::try_new(z)?;
                    self.zone.zp0 = zp;
                    self.zone.zp1 = zp;
                    self.zone.zp2 = zp;
                }
                op::SLOOP => {
                    let c = stack.get(a0)?;
                    if c < 0 {
                        return None;
                    } else {
                        loop_counter = (c as u32).min(0xFFFF);
                    }
                }
                op::RTG => self.round.mode = RoundMode::Grid,
                op::RTHG => self.round.mode = RoundMode::HalfGrid,
                op::SMD => state.graphics.min_distance = stack.get(a0)?,
                op::ELSE => {
                    let mut n = 1;
                    while n != 0 {
                        let next_ins = decoder.next()?.ok()?;
                        match next_ins.opcode {
                            op::IF => n += 1,
                            op::EIF => n -= 1,
                            _ => {}
                        }
                    }
                }
                op::SCVTCI => state.graphics.control_value_cutin = stack.get(a0)?,
                op::SSWCI => state.graphics.single_width_cutin = stack.get(a0)?,
                op::SSW => state.graphics.single_width = stack.get(a0)?,
                op::DUP => *stack.get_mut(a1)? = stack.get(a0)?,
                op::POP => {}
                op::CLEAR => new_top = 0,
                op::SWAP => {
                    let tmp = stack.get(a0)?;
                    *stack.get_mut(a0)? = stack.get(a1)?;
                    *stack.get_mut(a1)? = tmp;
                }
                op::DEPTH => *stack.get_mut(a0)? = stack_top as i32,
                op::CINDEX => {
                    let index = stack.get(a0)? as usize;
                    if a0 == 0 || index > a0 {
                        return None;
                    } else {
                        let v = stack.get(a0 - index)?;
                        *stack.get_mut(a0)? = v;
                    }
                }
                op::MINDEX => {
                    let index = stack.get(a0)? as usize;
                    if a0 == 0 || index > a0 {
                        return None;
                    } else {
                        let e = stack.get(a0 - index)?;
                        for i in (a0 - index)..(a0 - 1) {
                            let v = stack.get(i + 1)?;
                            *stack.get_mut(i)? = v;
                        }
                        *stack.get_mut(a0 - 1)? = e;
                    }
                }
                op::ALIGNPTS => {
                    let p1 = stack.get(a0)? as usize;
                    let p2 = stack.get(a1)? as usize;
                    let distance =
                        self.project(self.zone.zp0().point(p2)?, self.zone.zp1().point(p1)?) / 2;
                    self.move_point(self.zone.zp1, p1, distance);
                    self.move_point(self.zone.zp0, p2, -distance);
                }
                op::UTP => {
                    let point_ix = stack.get(a0)? as usize;
                    let marker = match (self.project.fv.x != 0, self.project.fv.y != 0) {
                        (true, true) => PointMarker::TOUCHED,
                        (true, false) => PointMarker::TOUCHED_X,
                        (false, true) => PointMarker::TOUCHED_Y,
                        (false, false) => PointMarker::default(),
                    };
                    self.zone
                        .zp0_mut()
                        .flags_mut(point_ix)?
                        .clear_marker(marker)
                }
                op::LOOPCALL | op::CALL => {
                    let (def_index, call_count) = if opcode == op::LOOPCALL {
                        (stack.get(a1)? as usize, stack.get(a0)?)
                    } else {
                        (stack.get(a0)? as usize, 1)
                    };
                    if callstack_top >= callstack_len {
                        return None;
                    }
                    if call_count > 0 {
                        let def = self.fdefs.get(def_index)?;
                        if !def.is_active {
                            return None;
                        }
                        let return_pc = ins.pc + 1;
                        let program = decoder.program;
                        let rec = CallRecord {
                            caller_program: program,
                            return_pc,
                            current_count: call_count as u32,
                            definition: def,
                        };
                        callstack[callstack_top] = rec;
                        callstack_top += 1;
                        decoder = Decoder::new(
                            def.program,
                            programs[def.program as usize],
                            def.offset as usize,
                        );
                    }
                }
                op::FDEF => {
                    let def_ix = stack.get(a0)? as usize;
                    if program == Program::Glyph || def_ix >= self.fdefs.len() {
                        return None;
                    }
                    let mut def = self.fdefs.get(def_ix)?;
                    def.is_active = true;
                    def.program = program;
                    def.offset = (ins.pc + 1) as u32;
                    def.end = def.offset;
                    while let Some(next_ins) = decoder.next() {
                        let next_ins = next_ins.ok()?;
                        match next_ins.opcode {
                            op::IDEF | op::FDEF => {
                                return None;
                            }
                            op::ENDF => {
                                def.end = decoder.pc as u32;
                                self.fdefs.set(def_ix, def)?;
                                break;
                            }
                            _ => {}
                        }
                    }
                    // decoder.next_pc += 1;
                }
                op::ENDF => {
                    if callstack_top == 0 {
                        return None;
                    }
                    let rec = callstack.get_mut(callstack_top - 1)?;
                    if rec.current_count > 1 {
                        rec.current_count -= 1;
                        decoder.pc = rec.definition.offset as usize;
                    } else {
                        decoder = Decoder::new(
                            rec.caller_program,
                            programs[rec.caller_program as usize],
                            rec.return_pc,
                        );
                        callstack_top -= 1;
                    }
                }
                op::MDAP0 | op::MDAP1 => {
                    let point = stack.get(a0)? as usize;
                    let mut distance = 0;
                    if (opcode & 1) != 0 {
                        let c = self.fast_project(self.zone.zp0().point(point)?);
                        distance = self.round(c) - c;
                    }
                    self.move_point(self.zone.zp0, point, distance)?;
                    rp0 = point;
                    rp1 = point;
                }
                op::IUP0 | op::IUP1 => {
                    let is_x = (opcode & 1) != 0;
                    let mut run = !self.zone.glyph.contours.is_empty();
                    if !self.v35 && self.compat {
                        if self.iupx && self.iupy {
                            run = false;
                        }
                        if is_x {
                            self.iupx = true;
                        } else {
                            self.iupy = true;
                        }
                    }
                    if run {
                        let marker = if is_x {
                            PointMarker::TOUCHED_X
                        } else {
                            PointMarker::TOUCHED_Y
                        };
                        let mut point = 0;
                        for i in 0..self.zone.glyph.contours.len() {
                            let mut end_point = self.zone.glyph.contour(i)? as usize;
                            let first_point = point;
                            if end_point >= self.zone.glyph.points.len() {
                                end_point = self.zone.glyph.points.len() - 1;
                            }
                            while point <= end_point
                                && !self.zone.glyph.flags(point)?.has_marker(marker)
                            {
                                point += 1;
                            }
                            if point <= end_point {
                                let first_touched = point;
                                let mut cur_touched = point;
                                point += 1;
                                while point <= end_point {
                                    if self.zone.glyph.flags(point)?.has_marker(marker) {
                                        self.zone.glyph.interpolate(
                                            is_x,
                                            cur_touched + 1,
                                            point - 1,
                                            cur_touched,
                                            point,
                                        )?;
                                        cur_touched = point;
                                    }
                                    point += 1;
                                }
                                if cur_touched == first_touched {
                                    self.zone.glyph.shift(
                                        is_x,
                                        first_point,
                                        end_point,
                                        cur_touched,
                                    )?;
                                } else {
                                    self.zone.glyph.interpolate(
                                        is_x,
                                        cur_touched + 1,
                                        end_point,
                                        cur_touched,
                                        first_touched,
                                    )?;
                                    if first_touched > 0 {
                                        self.zone.glyph.interpolate(
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
                    }
                }
                op::SHP0 | op::SHP1 => {
                    if stack_top < loop_counter as usize {
                        return None;
                    }
                    let (dx, dy, _, _) = self.compute_point_displacement(opcode, rp1, rp2)?;
                    while loop_counter > 0 {
                        args_top -= 1;
                        let index = stack.get(args_top)? as usize;
                        self.move_zp2_point(index, dx, dy, true)?;
                        loop_counter -= 1;
                    }
                    loop_counter = 1;
                    new_top = args_top;
                }
                op::SHC0 | op::SHC1 => {
                    let contour = stack.get(a0)? as usize;
                    let bound = if self.zone.zp2 == Zone::Twilight {
                        1
                    } else {
                        self.zone.zp2().contours.len()
                    };
                    if contour >= bound {
                        return None;
                    }
                    let (dx, dy, zone, index) =
                        self.compute_point_displacement(opcode, rp1, rp2)?;
                    let mut start = 0;
                    if contour != 0 {
                        let z = self.zone.zp2();
                        start = z.contour(contour - 1)? as usize + 1;
                    }
                    let limit = if self.zone.zp2 == Zone::Twilight {
                        self.zone.zp2().points.len()
                    } else {
                        let z = self.zone.zp2();
                        z.contour(contour)? as usize + 1
                    };
                    for i in start..limit {
                        if zone != self.zone.zp2 || index != i {
                            self.move_zp2_point(i, dx, dy, true)?;
                        }
                    }
                }
                op::SHZ0 | op::SHZ1 => {
                    if stack.get(a0)? >= 2 {
                        return None;
                    }
                    let (dx, dy, zone, index) =
                        self.compute_point_displacement(opcode, rp1, rp2)?;
                    let limit = if self.zone.zp2 == Zone::Twilight {
                        self.zone.zp2().points.len()
                    } else if self.zone.zp2 == Zone::Glyph && !self.zone.zp2().contours.is_empty() {
                        let z = self.zone.zp2();
                        *z.contours.last()? as usize + 1
                    } else {
                        0
                    };
                    for i in 0..limit {
                        if zone != self.zone.zp2 || i != index {
                            self.move_zp2_point(i, dx, dy, false)?;
                        }
                    }
                }
                op::SHPIX => {
                    if stack_top < loop_counter as usize + 1 {
                        return None;
                    }
                    let in_twilight = self.zone.zp0 == Zone::Twilight
                        || self.zone.zp1 == Zone::Twilight
                        || self.zone.zp2 == Zone::Twilight;
                    let a = stack.get(a0)?;
                    let dx = mul14(a, self.project.fv.x as i32);
                    let dy = mul14(a, self.project.fv.y as i32);
                    while loop_counter > 0 {
                        args_top -= 1;
                        let point = stack.get(args_top)? as usize;
                        if !self.v35 && self.compat {
                            if in_twilight
                                || (!(self.iupx && self.iupy)
                                    && ((composite && self.project.fv.y != 0)
                                        || self
                                            .zone
                                            .zp2()
                                            .flags
                                            .get(point)?
                                            .has_marker(PointMarker::TOUCHED_Y)))
                            {
                                self.move_zp2_point(point, dx, dy, true)?;
                            }
                        } else {
                            self.move_zp2_point(point, dx, dy, true)?;
                        }
                        loop_counter -= 1;
                    }
                    loop_counter = 1;
                    new_top = args_top;
                }
                op::IP => {
                    if stack_top < loop_counter as usize {
                        return None;
                    }
                    let in_twilight = self.zone.zp0 == Zone::Twilight
                        || self.zone.zp1 == Zone::Twilight
                        || self.zone.zp2 == Zone::Twilight;
                    let orus_base = if in_twilight {
                        self.zone.zp0().original(rp1)?
                    } else {
                        self.zone.zp0().unscaled(rp1)?
                    };
                    let cur_base = self.zone.zp0().point(rp1)?;
                    let old_range = if in_twilight {
                        self.dual_project(self.zone.zp1().original(rp2)?, orus_base)
                    } else {
                        self.dual_project(self.zone.zp1().unscaled(rp2)?, orus_base)
                    };
                    let cur_range = self.project(self.zone.zp1().point(rp2)?, cur_base);
                    while loop_counter > 0 {
                        loop_counter -= 1;
                        args_top -= 1;
                        let point = stack.get(args_top)? as usize;
                        let original_distance = if in_twilight {
                            self.dual_project(self.zone.zp2().original(point)?, orus_base)
                        } else {
                            self.dual_project(self.zone.zp2().unscaled(point)?, orus_base)
                        };
                        let cur_distance = self.project(self.zone.zp2().point(point)?, cur_base);
                        let mut new_distance = 0;
                        if original_distance != 0 {
                            if old_range != 0 {
                                new_distance = muldiv(original_distance, cur_range, old_range);
                            } else {
                                new_distance = original_distance;
                            }
                        }
                        self.move_point(self.zone.zp2, point, new_distance - cur_distance)?;
                    }
                    loop_counter = 1;
                    new_top = args_top;
                }
                op::MSIRP0 | op::MSIRP1 => {
                    let point = stack.get(args)? as usize;
                    if self.zone.zp1 == Zone::Twilight {
                        *self.zone.zp1_mut().point_mut(point)? = self.zone.zp0().original(rp0)?;
                        self.move_original(self.zone.zp1, point, stack.get(args + 1)?)?;
                        *self.zone.zp1_mut().point_mut(point)? = self.zone.zp1().original(point)?;
                    }
                    let d =
                        self.project(self.zone.zp1().point(point)?, self.zone.zp0().point(rp0)?);
                    let a = stack.get(args + 1)?;
                    self.move_point(self.zone.zp1, point, a.wrapping_sub(d))?;
                    rp1 = rp0;
                    rp2 = point;
                    if (opcode & 1) != 0 {
                        rp0 = point;
                    }
                }
                op::ALIGNRP => {
                    if stack_top < loop_counter as usize {
                        return None;
                    }
                    while loop_counter > 0 {
                        args_top -= 1;
                        let point = stack.get(args_top)? as usize;
                        let distance = self
                            .project(self.zone.zp1().point(point)?, self.zone.zp0().point(rp0)?);
                        self.move_point(self.zone.zp1, point, -distance)?;
                        loop_counter -= 1;
                    }
                    loop_counter = 1;
                    new_top = args_top;
                }
                op::RTDG => self.round.mode = RoundMode::DoubleGrid,
                op::MIAP0 | op::MIAP1 => {
                    let point_ix = stack.get(a0)? as usize;
                    let cvt_entry = stack.get(a1)? as usize;
                    let mut distance = self.cvt.get(cvt_entry)?;
                    if self.zone.zp0 == Zone::Twilight {
                        let fv = self.project.fv;
                        let z = self.zone.zp0_mut();
                        let original_point = z.original_mut(point_ix)?;
                        original_point.x = mul14(distance, fv.x as i32);
                        original_point.y = mul14(distance, fv.y as i32);
                        *z.point_mut(point_ix)? = *original_point;
                    }
                    let original_distance = self.fast_project(self.zone.zp0().point(point_ix)?);
                    if (opcode & 1) != 0 {
                        let delta = (distance - original_distance).abs();
                        if delta > state.graphics.control_value_cutin {
                            distance = original_distance;
                        }
                        distance = self.round(distance);
                    }
                    self.move_point(self.zone.zp0, point_ix, distance - original_distance)?;
                    rp0 = point_ix;
                    rp1 = point_ix;
                }
                op::WS => {
                    let index = stack.get(a0)? as usize;
                    self.store.set(index, stack.get(a1)?)?;
                }
                op::RS => {
                    let sp = stack.get_mut(a0)?;
                    let index = *sp as usize;
                    *sp = self.store.get(index)?;
                }
                op::WCVTP => {
                    let index = stack.get(a0)? as usize;
                    self.cvt.set(index, stack.get(a1)?)?;
                }
                op::WCVTF => {
                    let index = stack.get(a0)? as usize;
                    self.cvt.set(index, mul(stack.get(a1)?, self.scale))?;
                }
                op::RCVT => {
                    let sp = stack.get_mut(a0)?;
                    let index = *sp as usize;
                    // *sp = self.cvt.get(index).copied().unwrap_or(0);
                    *sp = self.cvt.get(index).unwrap_or(0);
                }
                op::GC0 | op::GC1 => {
                    let index = stack.get(a0)? as usize;
                    let r = if (opcode & 1) != 0 {
                        self.fast_dual_project(self.zone.zp2().original(index)?)
                    } else {
                        self.fast_project(self.zone.zp2().point(index)?)
                    };
                    *stack.get_mut(a0)? = r;
                }
                op::SCFS => {
                    let index = stack.get(a0)? as usize;
                    let a = self.fast_project(self.zone.zp2().point(index)?);
                    self.move_point(self.zone.zp2, index, stack.get(a1)?.wrapping_sub(a))?;
                    if self.zone.zp2 == Zone::Twilight {
                        *self.zone.twilight.original_mut(index)? =
                            self.zone.twilight.point(index)?;
                    }
                }
                op::MD0 | op::MD1 => {
                    let a = stack.get(a1)? as usize;
                    let b = stack.get(a0)? as usize;
                    let d = if (opcode & 1) != 0 {
                        self.project(self.zone.zp0().point(b)?, self.zone.zp1().point(a)?)
                    } else if self.zone.zp0 == Zone::Twilight || self.zone.zp1 == Zone::Twilight {
                        self.dual_project(
                            self.zone.zp0().original(b)?,
                            self.zone.zp1().original(a)?,
                        )
                    } else {
                        mul(
                            self.dual_project(
                                self.zone.zp0().unscaled(b)?,
                                self.zone.zp1().unscaled(a)?,
                            ),
                            self.yscale,
                        )
                    };
                    *stack.get_mut(a0)? = d;
                }
                op::MPPEM => {
                    *stack.get_mut(a0)? = self.ppem as i32;
                }
                op::MPS => {
                    *stack.get_mut(a0)? = if self.v35 {
                        self.ppem as i32
                    } else {
                        self.point_size
                    };
                }
                op::FLIPON => state.graphics.auto_flip = true,
                op::FLIPOFF => state.graphics.auto_flip = false,
                op::DEBUG => {}
                op::LT => *stack.get_mut(a0)? = (stack.get(a0)? < stack.get(a1)?) as i32,
                op::LTEQ => *stack.get_mut(a0)? = (stack.get(a0)? <= stack.get(a1)?) as i32,
                op::GT => *stack.get_mut(a0)? = (stack.get(a0)? > stack.get(a1)?) as i32,
                op::GTEQ => *stack.get_mut(a0)? = (stack.get(a0)? >= stack.get(a1)?) as i32,
                op::EQ => *stack.get_mut(a0)? = (stack.get(a0)? == stack.get(a1)?) as i32,
                op::NEQ => *stack.get_mut(a0)? = (stack.get(a0)? != stack.get(a1)?) as i32,
                op::ODD => *stack.get_mut(a0)? = (self.round(stack.get(a0)?) & 127 == 64) as i32,
                op::EVEN => *stack.get_mut(a0)? = (self.round(stack.get(a0)?) & 127 == 0) as i32,
                op::IF => {
                    if stack.get(a0)? == 0 {
                        let mut n = 1;
                        let mut out = false;
                        while !out {
                            let next_ins = decoder.next()?.ok()?;
                            match next_ins.opcode {
                                op::IF => n += 1,
                                op::ELSE => out = n == 1,
                                op::EIF => {
                                    n -= 1;
                                    out = n == 0;
                                }
                                _ => {}
                            }
                        }
                        // decoder.next_pc += 1;
                    }
                }
                op::EIF => {}
                op::AND => {
                    *stack.get_mut(a0)? = (stack.get(a0)? != 0 && stack.get(a1)? != 0) as i32
                }
                op::OR => *stack.get_mut(a0)? = (stack.get(a0)? != 0 || stack.get(a1)? != 0) as i32,
                op::NOT => *stack.get_mut(a0)? = (stack.get(a0)? == 0) as i32,
                op::SDB => state.graphics.delta_base = stack.get(a0)? as u16,
                op::SDS => state.graphics.delta_shift = (stack.get(a0)?).min(6) as u16,
                op::ADD => *stack.get_mut(a0)? += stack.get(a1)?,
                op::SUB => *stack.get_mut(a0)? -= stack.get(a1)?,
                op::DIV => {
                    let d = stack.get(a1)?;
                    if d == 0 {
                        return None;
                    }
                    let sp = stack.get_mut(a0)?;
                    *sp = muldiv_no_round(*sp, 64, d);
                }
                op::MUL => *stack.get_mut(a0)? = muldiv(stack.get(a0)?, stack.get(a1)?, 64),
                op::ABS => *stack.get_mut(a0)? = (stack.get(a0)?).abs(),
                op::NEG => *stack.get_mut(a0)? = -stack.get(a0)?,
                op::FLOOR => *stack.get_mut(a0)? = floor(stack.get(a0)?),
                op::CEILING => *stack.get_mut(a0)? = ceil(stack.get(a0)?),
                op::ROUND00..=op::ROUND11 => *stack.get_mut(a0)? = self.round(stack.get(a0)?),
                op::NROUND00..=op::NROUND11 => {}
                op::DELTAP1 | op::DELTAP2 | op::DELTAP3 => {
                    let p = self.ppem as u32;
                    let nump = stack.get(a0)? as u32;
                    let bias = match opcode {
                        op::DELTAP2 => 16,
                        op::DELTAP3 => 32,
                        _ => 0,
                    } + state.graphics.delta_base as u32;
                    for _ in 1..=nump {
                        if args_top < 2 {
                            return None;
                        }
                        args_top -= 2;
                        let a = stack.get(args_top + 1)? as usize;
                        if a >= self.zone.zp0().points.len() {
                            continue;
                        }
                        let mut b = stack.get(args_top)?;
                        let mut c = (b as u32 & 0xF0) >> 4;
                        c += bias;
                        if p == c {
                            b = (b & 0xF) - 8;
                            if b >= 0 {
                                b += 1;
                            }
                            b *= 1 << (6 - state.graphics.delta_shift as i32);
                            if !self.v35 && self.compat {
                                if !(self.iupx && self.iupy)
                                    && ((composite && self.project.fv.y != 0)
                                        || self
                                            .zone
                                            .zp0()
                                            .flags
                                            .get(a)?
                                            .has_marker(PointMarker::TOUCHED_Y))
                                {
                                    self.move_point(self.zone.zp0, a, b)?;
                                }
                            } else {
                                self.move_point(self.zone.zp0, a, b)?;
                            }
                        }
                    }
                    new_top = args_top;
                }
                op::DELTAC1 | op::DELTAC2 | op::DELTAC3 => {
                    let p = self.ppem as u32;
                    let nump = stack.get(args)? as u32;
                    let bias = match opcode {
                        op::DELTAC2 => 16,
                        op::DELTAC3 => 32,
                        _ => 0,
                    } + state.graphics.delta_base as u32;
                    for _ in 1..=nump {
                        if args_top < 2 {
                            return None;
                        }
                        args_top -= 2;
                        let a = stack.get(args_top + 1)? as usize;
                        let mut b = stack.get(args_top)?;
                        let mut c = (b as u32 & 0xF0) >> 4;
                        c += bias;
                        if p == c {
                            b = (b & 0xF) - 8;
                            if b >= 0 {
                                b += 1;
                            }
                            b *= 1 << (6 - state.graphics.delta_shift as i32);
                            let cvt_val = self.cvt.get(a)?;
                            self.cvt.set(a, cvt_val + b)?;
                        }
                    }
                    new_top = args_top;
                }
                op::SROUND | op::S45ROUND => {
                    let selector = stack.get(a0)?;
                    let grid_period = if opcode == op::SROUND {
                        self.round.mode = RoundMode::Super;
                        0x4000
                    } else {
                        self.round.mode = RoundMode::Super45;
                        0x2D41
                    };
                    match selector & 0xC0 {
                        0 => self.round.period = grid_period / 2,
                        0x40 => self.round.period = grid_period,
                        0x80 => self.round.period = grid_period * 2,
                        0xC0 => self.round.period = grid_period,
                        _ => {}
                    }
                    match selector & 0x30 {
                        0 => self.round.phase = 0,
                        0x10 => self.round.phase = self.round.period / 4,
                        0x20 => self.round.phase = self.round.period / 2,
                        0x30 => self.round.phase = self.round.period * 3 / 4,
                        _ => {}
                    }
                    if (selector & 0x0F) == 0 {
                        self.round.threshold = self.round.period - 1;
                    } else {
                        self.round.threshold = ((selector & 0x0F) - 4) * self.round.period / 8;
                    }
                    self.round.period >>= 8;
                    self.round.phase >>= 8;
                    self.round.threshold >>= 8;
                }
                op::JMPR | op::JROT | op::JROF => {
                    let cond = match opcode {
                        op::JROT => stack.get(a1)? != 0,
                        op::JROF => stack.get(a1)? == 0,
                        _ => true,
                    };
                    if cond {
                        let o = stack.get(a0)?;
                        if o == 0 && args == 0 {
                            return None;
                        }
                        if o < 0 {
                            decoder.pc = ins.pc - (-o) as usize;
                        } else {
                            decoder.pc = ins.pc + o as usize;
                        }
                        if callstack_top > 0
                            && decoder.pc > callstack[callstack_top - 1].definition.end as usize
                        {
                            return None;
                        }
                    }
                }
                op::ROFF => self.round.mode = RoundMode::Off,
                op::RUTG => self.round.mode = RoundMode::UpToGrid,
                op::RDTG => self.round.mode = RoundMode::DownToGrid,
                op::SANGW => {}
                op::AA => {}
                op::FLIPPT => {
                    if !self.v35 && self.compat && self.iupx && self.iupy {
                        // nothing
                    } else if stack_top < loop_counter as usize {
                        return None;
                    } else {
                        while loop_counter > 0 {
                            args_top -= 1;
                            let point = stack.get(args_top)? as usize;
                            self.zone.glyph.flags.get_mut(point)?.flip_on_curve();
                            loop_counter -= 1;
                        }
                    }
                    loop_counter = 1;
                    new_top = args_top;
                }
                op::FLIPRGON | op::FLIPRGOFF => {
                    if !self.v35 && self.compat && self.iupx && self.iupy {
                        // nothing
                    } else {
                        let a = stack.get(a1)? as usize;
                        let b = stack.get(a0)? as usize;
                        if b > a {
                            return None;
                        }
                        if opcode == op::FLIPRGON {
                            for tag in self.zone.glyph.flags.get_mut(b..=a)? {
                                tag.set_on_curve();
                            }
                        } else {
                            for tag in self.zone.glyph.flags.get_mut(b..=a)? {
                                tag.clear_on_curve();
                            }
                        }
                    }
                }
                op::SCANCTRL => {
                    let a = stack.get(a0)? as u16;
                    let b = a & 0xFF;
                    let scan_control = &mut state.graphics.scan_control;
                    if b == 0xFF {
                        *scan_control = true;
                    } else if b == 0 {
                        *scan_control = false;
                    } else {
                        if (a & 0x100) != 0 && self.ppem <= b {
                            *scan_control = true;
                        }
                        if (a & 0x200) != 0 && self.rotated {
                            *scan_control = true;
                        }
                        if (a & 0x800) != 0 && self.ppem > b {
                            *scan_control = false;
                        }
                        if (a & 0x1000) != 0 && self.rotated {
                            *scan_control = false;
                        }
                    }
                }
                op::SDPVTL0 | op::SDPVTL1 => {
                    let mut op = opcode;
                    let p1 = stack.get(a1)? as usize;
                    let p2 = stack.get(a0)? as usize;
                    let mut a;
                    let mut b;
                    {
                        let v1 = self.zone.zp1().original(p2)?;
                        let v2 = self.zone.zp2().original(p1)?;
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
                    let mut v = self.project.dv;
                    self.normalize(a, b, &mut v);
                    self.project.dv = v;
                    {
                        let v1 = self.zone.zp1().point(p2)?;
                        let v2 = self.zone.zp2().point(p1)?;
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
                    let mut v = self.project.pv;
                    self.normalize(a, b, &mut v);
                    self.project.pv = v;
                    self.project.update();
                }
                op::GETINFO => {
                    let a = stack.get(a0)?;
                    let mut k = 0;
                    if (a & 1) != 0 {
                        k = if self.v35 { 35 } else { 42 };
                    }
                    if (a & 2) != 0 && self.rotated {
                        k |= 1 << 8;
                    }
                    if (a & 8) != 0 && !self.coords.is_empty() {
                        k |= 1 << 10;
                    }
                    if (a & 32) != 0 && grayscale {
                        k |= 1 << 12;
                    }
                    if !self.v35 && self.subpixel {
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
                        if (a & 2048) != 0 && self.subpixel {
                            k |= 1 << 18;
                        }

                        if (a & 4096) != 0 && grayscale_cleartype {
                            k |= 1 << 19;
                        }
                    }
                    *stack.get_mut(a0)? = k;
                }
                op::IDEF => {
                    if program == Program::Glyph {
                        return None;
                    }
                    let def_ix = stack.get(args)? as usize;
                    let mut index = !0;
                    for i in 0..self.idefs.len() {
                        if !self.idefs.get(i)?.is_active {
                            index = i;
                            break;
                        }
                    }
                    if index == !0 {
                        return None;
                    }
                    let mut def = self.idefs.get(index)?;
                    def.program = program;
                    def.opcode = def_ix as u16;
                    def.offset = ins.pc as u32 + 1;
                    def.is_active = true;
                    while let Some(next_ins) = decoder.next() {
                        let next_ins = next_ins.ok()?;
                        match next_ins.opcode {
                            op::IDEF | op::FDEF => {
                                return None;
                            }
                            op::ENDF => {
                                def.end = decoder.pc as u32;
                                self.idefs.set(index, def)?;
                                break;
                            }
                            _ => {}
                        }
                    }
                    // decoder.next_pc += 1;
                }
                op::ROLL => {
                    let (a, b, c) = (stack.get(a2)?, stack.get(a1)?, stack.get(a0)?);
                    *stack.get_mut(a2)? = c;
                    *stack.get_mut(a1)? = a;
                    *stack.get_mut(a0)? = b;
                }
                op::MAX => *stack.get_mut(a0)? = (stack.get(a0)?).max(stack.get(a1)?),
                op::MIN => *stack.get_mut(a0)? = (stack.get(a0)?).min(stack.get(a1)?),
                op::SCANTYPE => {
                    let a = stack.get(a0)?;
                    if a >= 0 {
                        state.graphics.scan_type = a & 0xFFFF;
                    }
                }
                op::INSTCTRL => {
                    let a = stack.get(a1)? as u32;
                    let b = stack.get(a0)? as u32;
                    let af = 1 << (a - 1);
                    if !(1..=3).contains(&a) || (b != 0 && b != af) {
                        // nothing
                    } else {
                        state.graphics.instruct_control &= !(af as u8);
                        state.graphics.instruct_control |= b as u8;
                        if a == 3 && !self.v35 && state.mode != Hinting::VerticalSubpixel {
                            self.compat = b != 4;
                        }
                    }
                }
                op::PUSHB000..=op::PUSHW111 => {
                    let args = ins.arguments;
                    let push_count = args.len();
                    for (stack_value, value) in stack
                        .values
                        .get_mut(a0..a0 + push_count)?
                        .iter_mut()
                        .zip(args.values())
                    {
                        *stack_value = value;
                    }
                }
                op::NPUSHB | op::NPUSHW => {
                    let args = ins.arguments;
                    let push_len = args.len();
                    for (stack_value, value) in stack
                        .values
                        .get_mut(a0..a0 + push_len)?
                        .iter_mut()
                        .zip(args.values())
                    {
                        *stack_value = value;
                    }
                    new_top += push_len;
                }
                op::MDRP00000..=op::MDRP11111 => {
                    let point_ix = stack.get(args)? as usize;
                    let mut original_distance;
                    if self.zone.zp0 == Zone::Twilight || self.zone.zp1 == Zone::Twilight {
                        original_distance = self.dual_project(
                            self.zone.zp1().original(point_ix)?,
                            self.zone.zp0().original(rp0)?,
                        );
                    } else {
                        let v1 = self.zone.zp1().unscaled(point_ix)?;
                        let v2 = self.zone.zp0().unscaled(rp0)?;
                        original_distance = self.dual_project(v1, v2);
                        original_distance = mul(original_distance, self.yscale);
                    }
                    let cutin = state.graphics.single_width_cutin;
                    let value = state.graphics.single_width;
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
                        self.round(original_distance)
                    } else {
                        original_distance
                    };
                    let min_distance = state.graphics.min_distance;
                    if (opcode & 8) != 0 {
                        if original_distance >= 0 {
                            if distance < min_distance {
                                distance = min_distance;
                            }
                        } else if distance > -min_distance {
                            distance = -min_distance;
                        }
                    }
                    original_distance = self.project(
                        self.zone.zp1().point(point_ix)?,
                        self.zone.zp0().point(rp0)?,
                    );
                    self.move_point(
                        self.zone.zp1,
                        point_ix,
                        distance.wrapping_sub(original_distance),
                    )?;
                    rp1 = rp0;
                    rp2 = point_ix;
                    if (opcode & 16) != 0 {
                        rp0 = point_ix;
                    }
                }
                op::MIRP00000..=op::MIRP11111 => {
                    let point = stack.get(a0)? as usize;
                    let cvt_entry = (stack.get(a1)? + 1) as usize;
                    let mut cvt_distance = if cvt_entry == 0 {
                        0
                    } else {
                        self.cvt.get(cvt_entry - 1)?
                    };
                    let cutin = state.graphics.single_width_cutin;
                    let value = state.graphics.single_width;
                    let mut delta = (cvt_distance - value).abs();
                    if delta < cutin {
                        cvt_distance = if cvt_distance >= 0 { value } else { -value };
                    }
                    if self.zone.zp1 == Zone::Twilight {
                        let fv = self.project.fv;
                        let p = {
                            let p2 = self.zone.zp0().original(rp0)?;
                            let p1 = self.zone.zp1_mut().original_mut(point)?;
                            p1.x = p2.x + mul(cvt_distance, fv.x as i32);
                            p1.y = p2.y + mul(cvt_distance, fv.y as i32);
                            *p1
                        };
                        *self.zone.zp1_mut().point_mut(point)? = p;
                    }
                    let original_distance = self.dual_project(
                        self.zone.zp1().original(point)?,
                        self.zone.zp0().original(rp0)?,
                    );
                    let current_distance =
                        self.project(self.zone.zp1().point(point)?, self.zone.zp0().point(rp0)?);
                    if state.graphics.auto_flip && (original_distance ^ cvt_distance) < 0 {
                        cvt_distance = -cvt_distance;
                    }
                    let mut distance = if (opcode & 4) != 0 {
                        if self.zone.zp0 == self.zone.zp1 {
                            delta = (cvt_distance - original_distance).abs();
                            if delta > state.graphics.control_value_cutin {
                                cvt_distance = original_distance;
                            }
                        }
                        self.round(cvt_distance)
                    } else {
                        cvt_distance
                    };
                    let min_distance = state.graphics.min_distance;
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
                        self.zone.zp1,
                        point,
                        distance.wrapping_sub(current_distance),
                    )?;
                    rp1 = rp0;
                    if (opcode & 16) != 0 {
                        rp0 = point;
                    }
                    rp2 = point;
                }
                _ => {
                    let axis_count = self.axis_count as usize;
                    if axis_count != 0 && opcode == op::GETVAR {
                        if stack_top + axis_count < stack.values.len() {
                            if axis_count == self.coords.len() {
                                for (sp, coord) in stack
                                    .values
                                    .get_mut(a0..a0 + axis_count)?
                                    .iter_mut()
                                    .zip(self.coords)
                                {
                                    *sp = coord.to_bits() as i32;
                                }
                            } else {
                                for value in stack.values.get_mut(a0..a0 + axis_count)? {
                                    *value = 0;
                                }
                            }
                            new_top = stack_top + axis_count;
                        } else {
                            return None;
                        }
                    } else if axis_count != 0 && opcode == 0x92 {
                        *stack.get_mut(a0)? = 17;
                    } else {
                        let mut index = !0;
                        for i in 0..self.idefs.len() {
                            let idef = self.idefs.get(i)?;
                            if idef.is_active && idef.opcode == opcode as u16 {
                                index = i;
                                break;
                            }
                        }
                        if index != !0 && callstack_top < callstack_len {
                            let def = self.idefs.get(index)?;
                            let rec = CallRecord {
                                caller_program: program,
                                return_pc: ins.pc + 1,
                                current_count: count as u32,
                                definition: def,
                            };
                            callstack[callstack_top] = rec;
                            callstack_top += 1;
                            decoder = Decoder::new(
                                def.program,
                                programs[def.program as usize],
                                def.offset as usize,
                            );
                        } else {
                            return None;
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
            stack_top = new_top;
            if decoder.pc >= decoder.bytecode.len() {
                if callstack_top > 0 {
                    return None;
                }
                break;
            }
        }
        Some(count)
    }
}

pub struct Stack<'a> {
    values: &'a mut [i32],
}

impl<'a> Stack<'a> {
    pub fn new(values: &'a mut [i32]) -> Self {
        Self { values }
    }

    // fn get(&mut self, index: usize) -> Result<i32, HintError> {
    //     self.storage
    //         .get(index)
    //         .copied()
    //         .ok_or(HintError::InvalidStackReference)
    // }

    // fn get_mut(&mut self, index: usize) -> Result<&mut i32, HintError> {
    //     self.storage
    //         .get_mut(index)
    //         .ok_or(HintError::InvalidStackReference)
    // }

    fn get(&mut self, index: usize) -> Option<i32> {
        self.values.get(index).copied()
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut i32> {
        self.values.get_mut(index)
    }
}
