//! Managing the flow of control.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#managing-the-flow-of-control>

// 6 instructions

use super::{
    super::code::{opcodes as op, Decoder},
    Engine, HintErrorKind, OpResult,
};

impl<'a> Engine<'a> {
    /// If test.
    ///
    /// IF[] (0x58)
    ///
    /// Pops: e: stack element
    ///
    /// Tests the element popped off the stack: if it is zero (FALSE), the
    /// instruction pointer is jumped to the next ELSE or EIF instruction
    /// in the instruction stream. If the element at the top of the stack is
    /// nonzero (TRUE), the next instruction in the instruction stream is
    /// executed. Execution continues until an ELSE instruction is encountered
    /// or an EIF instruction ends the IF. If an else statement is found before
    /// the EIF, the instruction pointer is moved to the EIF statement.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#if-test>
    pub(super) fn op_if(&mut self, decoder: &mut Decoder) -> OpResult {
        if self.value_stack.pop()? == 0 {
            let mut nest_depth = 1;
            let mut out = false;
            while !out {
                let next_ins = decoder.next()?;
                match next_ins.opcode {
                    op::IF => nest_depth += 1,
                    op::ELSE => out = nest_depth == 1,
                    op::EIF => {
                        nest_depth -= 1;
                        out = nest_depth == 0;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Else.
    ///
    /// ELSE[] (0x1B)
    ///
    /// Marks the start of the sequence of instructions that are to be executed
    /// if an IF instruction encounters a FALSE value on the stack. This
    /// sequence of instructions is terminated with an EIF instruction.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#else>
    pub(super) fn op_else(&mut self, decoder: &mut Decoder) -> OpResult {
        let mut nest_depth = 1;
        while nest_depth != 0 {
            let next_ins = decoder.next()?;
            match next_ins.opcode {
                op::IF => nest_depth += 1,
                op::EIF => nest_depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }

    /// End if.
    ///
    /// EIF[] (0x59)
    ///
    /// Marks the end of an IF[] instruction.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#end-if>
    pub(super) fn op_eif(&mut self) -> OpResult {
        // Nothing
        Ok(())
    }

    /// Jump relative on true.
    ///
    /// JROT[] (0x78)
    ///
    /// Pops: e: stack element
    ///       offset: number of bytes to move the instruction pointer
    ///
    /// Pops and tests the element value, and then pops the offset. If the
    /// element value is non-zero (TRUE), the signed offset will be added
    /// to the instruction pointer and execution will be resumed at the address
    /// obtained. Otherwise, the jump is not taken and the next instruction in
    /// the instruction stream is executed. The jump is relative to the position
    /// of the instruction itself. That is, the instruction pointer is still
    /// pointing at the JROT[ ] instruction when offset is added to obtain the
    /// new address.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#jump-relative-on-true>
    pub(super) fn op_jrot(&mut self, decoder: &mut Decoder) -> OpResult {
        let e = self.value_stack.pop()?;
        self.do_jump(e != 0, decoder)
    }

    /// Jump.
    ///
    /// JMPR[] (0x1C)
    ///
    /// Pops: offset: number of bytes to move the instruction pointer
    ///
    /// The signed offset is added to the instruction pointer and execution
    /// is resumed at the new location in the instruction steam. The jump is
    /// relative to the position of the instruction itself. That is, the
    /// instruction pointer is still pointing at the JROT[] instruction when
    /// offset is added to obtain the new address.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#jump>
    pub(super) fn op_jmpr(&mut self, decoder: &mut Decoder) -> OpResult {
        self.do_jump(true, decoder)
    }

    /// Jump relative on false.
    ///
    /// JROF[] (0x78)
    ///
    /// Pops: e: stack element
    ///       offset: number of bytes to move the instruction pointer
    ///
    /// Pops and tests the element value, and then pops the offset. If the
    /// element value is non-zero (TRUE), the signed offset will be added
    /// to the instruction pointer and execution will be resumed at the address
    /// obtained. Otherwise, the jump is not taken and the next instruction in
    /// the instruction stream is executed. The jump is relative to the position
    /// of the instruction itself. That is, the instruction pointer is still
    /// pointing at the JROT[ ] instruction when offset is added to obtain the
    /// new address.
    ///
    /// Pops and tests the element value, and then pops the offset. If the
    /// element value is zero (FALSE), the signed offset will be added to the
    /// nstruction pointer and execution will be resumed at the address
    /// obtainted. Otherwise, the jump is not taken and the next instruction
    /// in the instruction stream is executed. The jump is relative to the
    /// position of the instruction itself. That is, the instruction pointer is
    /// still pointing at the JROT[ ] instruction when the offset is added to
    /// obtain the new address.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#jump-relative-on-false>
    pub(super) fn op_jrof(&mut self, decoder: &mut Decoder) -> OpResult {
        let e = self.value_stack.pop()?;
        self.do_jump(e == 0, decoder)
    }

    fn do_jump(&mut self, test: bool, decoder: &mut Decoder) -> OpResult {
        // Offset is relative to previous jump instruction and decoder is
        // already pointing to next instruction, so subtract one
        let jump_offset = self.value_stack.pop()? - 1;
        if test {
            if jump_offset == -1 {
                // If the offset is -1, we'll just loop in place...
                return Err(HintErrorKind::InvalidJump);
            }
            decoder.pc = decoder.pc.wrapping_add_signed(jump_offset as isize);
            if let Some(call_rec) = self.call_stack.peek() {
                // Don't allow jumping outside of our current function definition
                if !call_rec.definition.range().contains(&decoder.pc) {
                    return Err(HintErrorKind::InvalidJump);
                }
            }
        }
        Ok(())
    }
}
