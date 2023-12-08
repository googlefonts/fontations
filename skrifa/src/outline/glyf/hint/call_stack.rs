//! Tracking function call state.

use super::{
    code::{CodeDefinition, Program},
    error::HintErrorKind,
};

const CALL_STACK_SIZE: usize = 32;

#[derive(Copy, Clone, Default)]
pub struct CallRecord {
    pub caller_program: Program,
    pub return_pc: usize,
    pub current_count: u32,
    pub definition: CodeDefinition,
}

#[derive(Default)]
pub struct CallStack {
    records: [CallRecord; 32],
    top: usize,
}

impl CallStack {
    pub fn is_empty(&self) -> bool {
        self.top == 0
    }

    pub fn len(&self) -> usize {
        self.top
    }

    pub fn records(&self) -> &[CallRecord] {
        &self.records[..self.top]
    }

    pub fn push(&mut self, record: CallRecord) -> Result<(), HintErrorKind> {
        if self.top >= CALL_STACK_SIZE {
            return Err(HintErrorKind::CallStackOverflow);
        }
        self.records[self.top] = record;
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
