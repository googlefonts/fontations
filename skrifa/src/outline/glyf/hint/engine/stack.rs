//! Managing the stack and pushing data onto the interpreter stack.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#managing-the-stack>
//! and <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#pushing-data-onto-the-interpreter-stack>

// 26 instructions

use super::{super::code::Arguments, Engine, OpResult};

impl<'a> Engine<'a> {
    /// Duplicate top stack element.
    ///
    /// DUP[] (0x20)
    ///
    /// Pops: e
    /// Pushes: e, e
    ///
    /// Duplicates the element at the top of the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#duplicate-top-stack-element>
    pub(super) fn op_dup(&mut self) -> OpResult {
        self.value_stack.dup()
    }

    /// Pop top stack element.
    ///
    /// POP[] (0x21)
    ///
    /// Pops: e
    ///
    /// Pops the top element of the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#pop-top-stack-element>
    pub(super) fn op_pop(&mut self) -> OpResult {
        self.value_stack.pop()?;
        Ok(())
    }

    /// Clear the entire stack.
    ///
    /// CLEAR[] (0x22)
    ///
    /// Pops: all the items on the stack
    ///
    /// Clears all elements from the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#clear-the-entire-stack>
    pub(super) fn op_clear(&mut self) -> OpResult {
        self.value_stack.clear();
        Ok(())
    }

    /// Swap the top two elements on the stack.
    ///
    /// SWAP[] (0x23)
    ///
    /// Pops: e2, e1
    /// Pushes: e1, e2
    ///
    /// Swaps the top two elements of the stack making the old top element the
    /// second from the top and the old second element the top element.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#swap-the-top-two-elements-on-the-stack>
    pub(super) fn op_swap(&mut self) -> OpResult {
        self.value_stack.swap()
    }

    /// Returns the depth of the stack.
    ///
    /// DEPTH[] (0x24)
    ///
    /// Pushes: n; number of elements
    ///
    /// Pushes n, the number of elements currently in the stack onto the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#returns-the-depth-of-the-stack>
    pub(super) fn op_depth(&mut self) -> OpResult {
        let n = self.value_stack.len();
        self.value_stack.push(n as i32)
    }

    /// Copy the indexed element to the top of the stack.
    ///
    /// CINDEX[] (0x25)
    ///
    /// Pops: k: stack element number
    /// Pushes: ek: indexed element
    ///
    /// Puts a copy of the kth stack element on the top of the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#copy-the-indexed-element-to-the-top-of-the-stack>
    pub(super) fn op_cindex(&mut self) -> OpResult {
        self.value_stack.copy_index()
    }

    /// Move the indexed element to the top of the stack.
    ///
    /// MINDEX[] (0x26)
    ///
    /// Pops: k: stack element number
    /// Pushes: ek: indexed element
    ///
    /// Moves the indexed element to the top of the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#move-the-indexed-element-to-the-top-of-the-stack>
    pub(super) fn op_mindex(&mut self) -> OpResult {
        self.value_stack.move_index()
    }

    /// Roll the top three stack elements.
    ///
    /// ROLL[] (0x8a)
    ///
    /// Pops: a, b, c (top three stack elements)
    /// Pushes: b, a, c (elements reordered)
    ///
    /// Performs a circular shift of the top three objects on the stack with
    /// the effect being to move the third element to the top of the stack
    /// and to move the first two elements down one position. ROLL is
    /// equivalent to MINDEX[] 3.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#roll-the-top-three-stack-elements>
    pub(super) fn op_roll(&mut self) -> OpResult {
        self.value_stack.roll()
    }

    /// Push data onto the interpreter stack.
    ///
    /// NPUSHB[] (0x8a)
    ///
    /// Takes n unsigned bytes from the instruction stream, where n is an
    /// unsigned integer in the range (0..255), and pushes them onto the stack.
    /// n itself is not pushed onto the stack.
    ///
    /// NPUSHW[] (0x41)
    ///
    /// Takes n 16-bit signed words from the instruction stream, where n is an
    /// unsigned integer in the range (0..255), and pushes them onto the stack.
    /// n itself is not pushed onto the stack.
    ///
    /// PUSHB[abc] (0xB0 - 0xB7)
    ///
    /// Takes the specified number of bytes from the instruction stream and
    /// pushes them onto the interpreter stack.
    /// The variables a, b, and c are binary digits representing numbers from
    /// 000 to 111 (0-7 in binary). Because the actual number of bytes (n) is
    /// from 1 to 8, 1 is automatically added to the ABC figure to obtain the
    /// actual number of bytes pushed.
    ///
    /// PUSHW[abc] (0xB8 - 0xBF)
    ///
    /// Takes the specified number of words from the instruction stream and
    /// pushes them onto the interpreter stack.
    /// The variables a, b, and c are binary digits representing numbers from
    /// 000 to 111 (0-7 binary). Because the actual number of bytes (n) is from
    /// 1 to 8, 1 is automatically added to the abc figure to obtain the actual
    /// number of bytes pushed.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#pushing-data-onto-the-interpreter-stack>    
    pub(super) fn op_push(&mut self, args: &Arguments) -> OpResult {
        self.value_stack.push_args(args)
    }
}
