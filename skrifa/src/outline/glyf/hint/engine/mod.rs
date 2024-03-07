//! TrueType bytecode interpreter.

mod arith;
mod control_flow;
mod cvt;
mod data;
mod definition;
mod delta;
mod dispatch;
mod graphics_state;
mod logical;
mod misc;
mod outline;
mod round;
mod stack;
mod storage;

use read_fonts::{
    tables::glyf::bytecode::Instruction,
    types::{F26Dot6, F2Dot14, Point},
};

use super::{
    super::Outlines,
    cvt::Cvt,
    definition::DefinitionState,
    error::{HintError, HintErrorKind},
    graphics_state::GraphicsState,
    program::ProgramState,
    storage::Storage,
    value_stack::ValueStack,
};

pub type OpResult = Result<(), HintErrorKind>;

/// TrueType bytecode interpreter.
pub struct Engine<'a> {
    program: ProgramState<'a>,
    graphics_state: GraphicsState<'a>,
    definitions: DefinitionState<'a>,
    cvt: Cvt<'a>,
    storage: Storage<'a>,
    value_stack: ValueStack<'a>,
    loop_budget: LoopBudget,
    axis_count: u16,
    coords: &'a [F2Dot14],
    is_composite: bool,
}

/// Tracks budgets for loops to limit execution time.
struct LoopBudget {
    /// Maximum number of times we can do backward jumps or
    /// loop calls.
    limit: usize,
    /// Current number of backward jumps executed.
    backward_jumps: usize,
    /// Current number of loop call iterations executed.
    loop_calls: usize,
}

impl LoopBudget {
    fn new(outlines: &Outlines, point_count: Option<usize>) -> Self {
        // Compute limits for loop calls and backward jumps.
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6955>
        let mut limit = if let Some(point_count) = point_count {
            (point_count * 10).max(50) + (outlines.cvt.len() / 10).max(50)
        } else {
            300 + 22 * outlines.cvt.len()
        };
        limit = limit.min(100 * outlines.glyph_count());
        // FreeType has two variables for neg_jump_counter_max and
        // loopcall_counter_max but sets them to the same value so
        // we'll just use a single limit.
        Self {
            limit,
            backward_jumps: 0,
            loop_calls: 0,
        }
    }

    fn doing_backward_jump(&mut self) -> Result<(), HintErrorKind> {
        self.backward_jumps += 1;
        if self.backward_jumps > self.limit {
            Err(HintErrorKind::ExceededExecutionBudget)
        } else {
            Ok(())
        }
    }

    fn doing_loop_call(&mut self, count: usize) -> Result<(), HintErrorKind> {
        self.loop_calls += count;
        if self.loop_calls > self.limit {
            Err(HintErrorKind::ExceededExecutionBudget)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
use mock::MockEngine;

#[cfg(test)]
mod mock {
    use super::{
        super::{
            cow_slice::CowSlice,
            definition::{Definition, DefinitionMap, DefinitionState},
            graphics_state::Zone,
            program::{Program, ProgramState},
            Point, PointFlags,
        },
        Engine, F26Dot6, GraphicsState, LoopBudget, ValueStack,
    };

    /// Mock engine for testing.
    pub(super) struct MockEngine {
        cvt_storage: Vec<i32>,
        value_stack: Vec<i32>,
        definitions: Vec<Definition>,
        unscaled: Vec<Point<i32>>,
        points: Vec<Point<F26Dot6>>,
        point_flags: Vec<PointFlags>,
        contours: Vec<u16>,
    }

    impl MockEngine {
        pub fn new() -> Self {
            Self {
                cvt_storage: vec![0; 32],
                value_stack: vec![0; 32],
                definitions: vec![Default::default(); 8],
                unscaled: vec![Default::default(); 32],
                points: vec![Default::default(); 64],
                point_flags: vec![Default::default(); 32],
                contours: vec![32],
            }
        }

        pub fn engine(&mut self) -> Engine {
            let font_code = &[];
            let cv_code = &[];
            let glyph_code = &[];
            let (cvt, storage) = self.cvt_storage.split_at_mut(16);
            let (function_defs, instruction_defs) = self.definitions.split_at_mut(5);
            let definition = DefinitionState::new(
                DefinitionMap::Mut(function_defs),
                DefinitionMap::Mut(instruction_defs),
            );
            let (points, original) = self.points.split_at_mut(32);
            let glyph_zone = Zone::new(
                &mut self.unscaled,
                original,
                points,
                &mut self.point_flags,
                &self.contours,
            );
            let mut graphics_state = GraphicsState::default();
            graphics_state.zones[1] = glyph_zone;
            Engine {
                graphics_state,
                cvt: CowSlice::new_mut(cvt).into(),
                storage: CowSlice::new_mut(storage).into(),
                value_stack: ValueStack::new(&mut self.value_stack),
                program: ProgramState::new(font_code, cv_code, glyph_code, Program::Font),
                loop_budget: LoopBudget {
                    limit: 10,
                    backward_jumps: 0,
                    loop_calls: 0,
                },
                definitions: definition,
                axis_count: 0,
                coords: &[],
                is_composite: false,
            }
        }
    }

    impl Default for MockEngine {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<'a> Engine<'a> {
        /// Helper to push values to the stack, invoke a callback and check
        /// the expected result.    
        pub(super) fn test_exec(
            &mut self,
            push: &[i32],
            expected_result: impl Into<i32>,
            mut f: impl FnMut(&mut Engine),
        ) {
            for &val in push {
                self.value_stack.push(val).unwrap();
            }
            f(self);
            assert_eq!(self.value_stack.pop().ok(), Some(expected_result.into()));
        }
    }
}
