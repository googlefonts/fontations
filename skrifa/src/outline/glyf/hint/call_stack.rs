//! Tracking function call state.

use super::{definition::Definition, error::HintErrorKind, program::Program};

// FreeType provides a call stack with a depth of 32.
// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L502>
const MAX_DEPTH: usize = 32;

/// Record of an active invocation of a function or instruction
/// definition.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.h#L90>
#[derive(Copy, Clone, Default)]
pub struct CallRecord {
    pub caller_program: Program,
    pub return_pc: usize,
    pub current_count: u32,
    pub definition: Definition,
}

/// Tracker for nested active function or instruction calls.
#[derive(Default)]
pub struct CallStack {
    records: [CallRecord; MAX_DEPTH],
    top: usize,
}

impl CallStack {
    pub fn len(&self) -> usize {
        self.top
    }

    pub fn is_empty(&self) -> bool {
        self.top == 0
    }

    pub fn records(&self) -> &[CallRecord] {
        &self.records[..self.top]
    }

    pub fn push(&mut self, record: CallRecord) -> Result<(), HintErrorKind> {
        let top = self
            .records
            .get_mut(self.top)
            .ok_or(HintErrorKind::CallStackOverflow)?;
        *top = record;
        self.top += 1;
        Ok(())
    }

    pub fn peek(&self) -> Option<&CallRecord> {
        self.records.get(self.top.checked_sub(1)?)
    }

    pub fn pop(&mut self) -> Result<CallRecord, HintErrorKind> {
        let record = *self.peek().ok_or(HintErrorKind::CallStackUnderflow)?;
        self.top -= 1;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_ops() {
        let mut stack = CallStack::default();
        for i in 0..32 {
            stack
                .push(CallRecord {
                    caller_program: Program::Glyph,
                    return_pc: 0,
                    current_count: 1,
                    definition: Definition::new(Program::Font, 0..i, i as i32),
                })
                .unwrap();
        }
        assert!(matches!(
            stack.push(CallRecord::default()),
            Err(HintErrorKind::CallStackOverflow)
        ));
        assert_eq!(stack.peek().unwrap().definition.key(), 31);
        for i in (0..32).rev() {
            assert_eq!(stack.pop().unwrap().definition.key(), i);
        }
        assert!(matches!(
            stack.pop(),
            Err(HintErrorKind::CallStackUnderflow)
        ));
    }
}
