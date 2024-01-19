//! TrueType interpreter engine.

mod arith;
mod control_flow;
mod cvt;
mod dispatch;
mod logical;
mod stack;
mod storage;

use crate::outline::glyf::Outlines;
use crate::prelude::NormalizedCoord;

use super::call_stack::{CallRecord, CallStack};
use super::code::{CodeDefinition, CodeDefinitionSlice, Decoder, Instruction, Program};
use super::error::HintErrorKind;
use super::graphics_state::{CoordAxis, GraphicsState, RetainedGraphicsState, RoundMode};
use super::value_stack::ValueStack;
use super::InstanceState;
use super::{math::*, HintError};
use super::{Cvt, EmbeddedHinting, Storage, Zone, ZonePointer};

pub type Point = super::Point<i32>;
pub type OpResult = Result<(), HintErrorKind>;

pub const TRACE: bool = false;

pub struct LoopLimitTracker {
    max_loop_calls: usize,
    max_backward_jumps: usize,
    loop_calls: usize,
    backward_jumps: usize,
}

impl LoopLimitTracker {
    pub fn new(outlines: &Outlines, point_count: Option<usize>) -> Self {
        // Compute limits for loop calls and backward jumps.
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6955>
        let mut max_loop_calls = if let Some(point_count) = point_count {
            (point_count * 10).max(50) + (outlines.cvt.len() / 10).max(50)
        } else {
            300 + 22 * outlines.cvt.len()
        };
        max_loop_calls = max_loop_calls.min(100 * outlines.glyph_count());
        Self {
            max_loop_calls,
            max_backward_jumps: max_loop_calls,
            loop_calls: 0,
            backward_jumps: 0,
        }
    }
}

/// TrueType hinting engine.
pub struct Engine<'a> {
    value_stack: ValueStack<'a>,
    storage: Storage<'a>,
    cvt: Cvt<'a>,
    function_defs: CodeDefinitionSlice<'a>,
    instruction_defs: CodeDefinitionSlice<'a>,
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
    backward_compat_enabled: bool,
    is_pedantic: bool,
}

impl<'a> Engine<'a> {
    /// Creates a new hinting engine.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value_stack: ValueStack<'a>,
        storage: impl Into<Storage<'a>>,
        cvt: impl Into<Cvt<'a>>,
        function_defs: CodeDefinitionSlice<'a>,
        instruction_defs: CodeDefinitionSlice<'a>,
        twilight: Zone<'a>,
        glyph: Zone<'a>,
        coords: &'a [NormalizedCoord],
        axis_count: u16,
    ) -> Self {
        Self {
            value_stack,
            call_stack: CallStack::default(),
            storage: storage.into(),
            cvt: cvt.into(),
            function_defs,
            instruction_defs,
            coords,
            axis_count,
            instance: InstanceState::default(),
            graphics: GraphicsState {
                zones: [twilight, glyph],
                ..Default::default()
            },
            y_scale: 0,
            is_composite: false,
            is_rotated: false,
            did_iup_x: false,
            did_iup_y: false,
            is_v35: false,
            backward_compat_enabled: false,
            is_pedantic: false,
        }
    }

    pub fn run_font_program<'b>(
        &mut self,
        state: &'b mut InstanceState,
        fpgm: &'a [u8],
    ) -> Result<u32, HintError> {
        let programs = [fpgm, &[], &[]];
        self.function_defs.reset();
        self.instruction_defs.reset();
        state.ppem = 0;
        state.scale = 0;
        state.mode = EmbeddedHinting::default();
        state.graphics = RetainedGraphicsState::default();
        self.graphics = GraphicsState::default();
        let res = self.execute_all(programs, Program::Font, false);
        if res.is_err() {
            println!("{res:?}");
        }
        res
    }

    pub fn run_cv_program<'b>(
        &mut self,
        state: &'b mut InstanceState,
        mode: EmbeddedHinting,
        fpgm: &'a [u8],
        prep: &'a [u8],
        ppem: u16,
        scale: i32,
    ) -> bool {
        let programs = [fpgm, prep, &[]];
        self.graphics.zone_mut(ZonePointer::Twilight).clear();
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

    pub fn run_glyph_program<'b>(
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
        zone: ZonePointer,
        point_ix: usize,
        distance: i32,
    ) -> Result<(), HintErrorKind> {
        let fdotp = self.graphics.fdotp;
        let fv = self.graphics.freedom_vector;
        let fv_axes = self.graphics.freedom_axis;
        let point = self.graphics.zone_mut(zone).original_mut(point_ix)?;
        match fv_axes {
            CoordAxis::X => point.x += distance,
            CoordAxis::Y => point.y += distance,
            CoordAxis::Both => {
                if fv.x != 0 {
                    point.x += mul_div(distance, fv.x, fdotp);
                }
                if fv.y != 0 {
                    point.y += mul_div(distance, fv.y, fdotp);
                }
            }
        }
        Ok(())
    }

    fn move_point(
        &mut self,
        zone: ZonePointer,
        point_ix: usize,
        distance: i32,
    ) -> Result<(), HintErrorKind> {
        let legacy = self.is_v35;
        let bc = self.backward_compat_enabled;
        let iupx = self.did_iup_x;
        let iupy = self.did_iup_y;
        let fdotp = self.graphics.fdotp;
        let fv = self.graphics.freedom_vector;
        let fv_axes = self.graphics.freedom_axis;
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
                        point.x += mul_div(distance, fv.x, fdotp);
                    }
                    zone.touch(point_ix, CoordAxis::X)?;
                }
                if fv.y != 0 {
                    if !(!legacy && bc && iupx && iupy) {
                        zone.point_mut(point_ix)?.y += mul_div(distance, fv.y, fdotp);
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
        let dx = mul_div(distance, fv.x, fdotp);
        let dy = mul_div(distance, fv.y, fdotp);
        Ok(PointDisplacement {
            zone,
            point_ix,
            dx,
            dy,
        })
    }
}

struct PointDisplacement {
    pub zone: ZonePointer,
    pub point_ix: usize,
    pub dx: i32,
    pub dy: i32,
}

impl<'a> Engine<'a> {
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
        self.is_v35 = false;
        if self.instance.mode.retain_linear_metrics() {
            self.backward_compat_enabled = true;
        } else if self.instance.mode.is_antialiased() {
            self.backward_compat_enabled = (self.graphics.instruct_control & 0x4) == 0;
        } else {
            self.backward_compat_enabled = false;
        }
        self.is_composite = is_composite;
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
                        kind,
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
