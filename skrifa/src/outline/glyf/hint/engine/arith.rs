//! Arithmetic and math instructions.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#arithmetic-and-math-instructions>

// 10 instructions

use super::{
    super::math::{ceil, floor, mul_div, mul_div_no_round},
    super::HintErrorKind,
    Engine, OpResult,
};

impl<'a> Engine<'a> {
    /// ADD[] (0x60)
    ///
    /// Pops: n1, n2 (F26Dot6)
    /// Pushes: (n2 + n1)
    ///
    /// Pops n1 and n2 off the stack and pushes the sum of the two elements
    /// onto the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#add>
    pub(super) fn op_add(&mut self) -> OpResult {
        self.value_stack.apply_binary(|a, b| Ok(a.wrapping_add(b)))
    }

    /// SUB[] (0x61)
    ///
    /// Pops: n1, n2 (F26Dot6)
    /// Pushes: (n2 - n1)
    ///
    /// Pops n1 and n2 off the stack and pushes the difference of the two
    /// elements onto the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#subtract>
    pub(super) fn op_sub(&mut self) -> OpResult {
        self.value_stack.apply_binary(|a, b| Ok(a.wrapping_sub(b)))
    }

    /// DIV[] (0x62)
    ///
    /// Pops: n1, n2 (F26Dot6)
    /// Pushes: (n2/n1)
    ///
    /// Pops n1 and n2 off the stack and pushes onto the stack the quotient
    /// obtained by dividing n2 by n1. Note that this truncates rather than
    /// rounds the value.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#divide>
    pub(super) fn op_div(&mut self) -> OpResult {
        self.value_stack.apply_binary(|a, b| {
            if b == 0 {
                Err(HintErrorKind::DivideByZero)
            } else {
                Ok(mul_div_no_round(a, 64, b))
            }
        })
    }

    /// MUL[] (0x63)
    ///
    /// Pops: n1, n2 (F26Dot6)
    /// Pushes: (n2 * n1)
    ///
    /// Pops n1 and n2 off the stack and pushes onto the stack the product of
    /// the two elements.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#multiply>
    pub(super) fn op_mul(&mut self) -> OpResult {
        self.value_stack.apply_binary(|a, b| Ok(mul_div(a, b, 64)))
    }

    /// ABS[] (0x64)
    ///
    /// Pops: n
    /// Pushes: |n|: absolute value of n (F26Dot6)
    ///
    /// Pops n off the stack and pushes onto the stack the absolute value of n.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#absolute-value>
    pub(super) fn op_abs(&mut self) -> OpResult {
        self.value_stack.apply_unary(|n| Ok(n.wrapping_abs()))
    }

    /// NEG[] (0x65)
    ///
    /// Pops: n1
    /// Pushes: -n1: negation of n1 (F26Dot6)
    ///
    /// This instruction pops n1 off the stack and pushes onto the stack the
    /// negated value of n1.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#negate>
    pub(super) fn op_neg(&mut self) -> OpResult {
        self.value_stack.apply_unary(|n1| Ok(n1.wrapping_neg()))
    }

    /// FLOOR[] (0x66)
    ///
    /// Pops: n1: number whose floor is desired (F26Dot6)
    /// Pushes: n: floor of n1 (F26Dot6)
    ///
    /// Pops n1 and returns n, the greatest integer value less than or equal to n1.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#floor>
    pub(super) fn op_floor(&mut self) -> OpResult {
        self.value_stack.apply_unary(|n1| Ok(floor(n1)))
    }

    /// CEILING[] (0x67)
    ///
    /// Pops: n1: number whose ceiling is desired (F26Dot6)
    /// Pushes: n: ceiling of n1 (F26Dot6)
    ///
    /// Pops n1 and returns n, the least integer value greater than or equal to n1.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#ceiling>
    pub(super) fn op_ceiling(&mut self) -> OpResult {
        self.value_stack.apply_unary(|n1| Ok(ceil(n1)))
    }

    /// MAX[] (0x8B)
    ///
    /// Pops: e1, e2
    /// Pushes: maximum of e1 and e2
    ///
    /// Pops two elements, e1 and e2, from the stack and pushes the larger of
    /// these two quantities onto the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#maximum-of-top-two-stack-elements>
    pub(super) fn op_max(&mut self) -> OpResult {
        self.value_stack.apply_binary(|a, b| Ok(a.max(b)))
    }

    /// MIN[] (0x8C)
    ///
    /// Pops: e1, e2
    /// Pushes: minimum of e1 and e2
    ///
    /// Pops two elements, e1 and e2, from the stack and pushes the smaller
    /// of these two quantities onto the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#minimum-of-top-two-stack-elements>
    pub(super) fn op_min(&mut self) -> OpResult {
        self.value_stack.apply_binary(|a, b| Ok(a.min(b)))
    }
}
