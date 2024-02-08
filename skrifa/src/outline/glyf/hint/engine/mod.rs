//! TrueType bytecode interpreter.

mod arith;
mod control_flow;
mod dispatch;
mod graphics_state;
mod logical;
mod stack;

use read_fonts::tables::glyf::bytecode::{Decoder, Instruction};

use super::{
    super::Outlines,
    code_state::ProgramKind,
    error::{HintError, HintErrorKind},
    graphics_state::GraphicsState,
    value_stack::ValueStack,
};

pub type OpResult = Result<(), HintErrorKind>;

/// TrueType bytecode interpreter.
pub struct Engine<'a> {
    graphics_state: GraphicsState<'a>,
    value_stack: ValueStack<'a>,
    initial_program: ProgramKind,
    decoder: Decoder<'a>,
    loop_budget: LoopBudget,
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
    use super::{Decoder, Engine, GraphicsState, LoopBudget, ProgramKind, ValueStack};

    /// Mock engine for testing.
    pub(super) struct MockEngine {
        value_stack: Vec<i32>,
    }

    impl MockEngine {
        pub fn new() -> Self {
            Self {
                value_stack: vec![0; 32],
            }
        }

        pub fn engine(&mut self) -> Engine {
            Engine {
                graphics_state: GraphicsState::default(),
                value_stack: ValueStack::new(&mut self.value_stack),
                initial_program: ProgramKind::Font,
                decoder: Decoder::new(&[], 0),
                loop_budget: LoopBudget {
                    limit: 10,
                    backward_jumps: 0,
                    loop_calls: 0,
                },
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
