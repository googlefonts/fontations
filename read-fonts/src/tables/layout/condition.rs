//! Shared evaluation of layout and VARC conditions.

use super::{Condition, ConditionSet, LookupConditionRecord};
use crate::limits::MAX_RECURSION_DEPTH;
use crate::tables::variations::NO_VARIATION_INDEX;
use crate::types::F2Dot14;
use crate::{FontData, FontRead, ReadError};

#[cfg(test)]
#[path = "condition_tests.rs"]
mod tests;

/// An error encountered when evaluating a condition tree.
#[derive(Clone, Debug, PartialEq)]
pub enum ConditionError<E> {
    /// A condition or its offset array could not be read.
    Read(ReadError),
    /// The condition exceeded the recursion or total-work limit.
    LimitExceeded,
    /// The caller could not resolve a variation delta.
    Delta(E),
}

impl<E: core::fmt::Display> core::fmt::Display for ConditionError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Read(error) => error.fmt(f),
            Self::LimitExceeded => f.write_str("condition evaluation limit exceeded"),
            Self::Delta(error) => error.fmt(f),
        }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for ConditionError<E> {}

impl Condition<'_> {
    /// Evaluates this condition at the given normalized coordinates.
    ///
    /// Missing coordinates are zero. `delta` resolves format-2 variation
    /// indices using the caller's variation store; it is not called for
    /// [`NO_VARIATION_INDEX`]. The value is tested before rounding.
    ///
    /// Malformed children and evaluation limits return errors, including
    /// beneath a NOT condition. AND and OR conditions short-circuit.
    pub fn evaluate<E>(
        &self,
        coords: &[F2Dot14],
        delta: impl FnMut(u32) -> Result<f64, E>,
    ) -> Result<bool, ConditionError<E>> {
        Evaluator::new(coords, delta).evaluate(self, MAX_RECURSION_DEPTH)
    }
}

impl ConditionSet<'_> {
    /// Evaluates all conditions in this set, sharing one total-work budget.
    ///
    /// A null condition offset is unconditional. See [`Condition::evaluate`]
    /// for variation-delta resolution and error handling.
    pub fn evaluate<E>(
        &self,
        coords: &[F2Dot14],
        delta: impl FnMut(u32) -> Result<f64, E>,
    ) -> Result<bool, ConditionError<E>> {
        let offsets = self.condition_offsets();
        if offsets.len() != usize::from(self.condition_count()) {
            return Err(ConditionError::Read(ReadError::OutOfBounds));
        }
        let mut evaluator = Evaluator::new(coords, delta);
        for offset in offsets {
            if !evaluator.evaluate_offset(
                self.offset_data(),
                offset.get().to_u32(),
                MAX_RECURSION_DEPTH,
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl LookupConditionRecord {
    /// Evaluates this record's condition, relative to its FeatureLookups table.
    ///
    /// A null offset is unconditional. See [`Condition::evaluate`] for
    /// variation-delta resolution and error handling.
    pub fn evaluate<E>(
        &self,
        data: FontData<'_>,
        coords: &[F2Dot14],
        delta: impl FnMut(u32) -> Result<f64, E>,
    ) -> Result<bool, ConditionError<E>> {
        Evaluator::new(coords, delta).evaluate_offset(
            data,
            self.condition_offset().offset().to_u32(),
            MAX_RECURSION_DEPTH,
        )
    }
}

struct Evaluator<'a, F> {
    coords: &'a [F2Dot14],
    delta: F,
    remaining_ops: usize,
}

impl<'a, E, F: FnMut(u32) -> Result<f64, E>> Evaluator<'a, F> {
    fn new(coords: &'a [F2Dot14], delta: F) -> Self {
        Self {
            coords,
            delta,
            // Enough for a full flat ConditionSet, while bounding repeated
            // visits when several offsets refer to the same child.
            remaining_ops: usize::from(u16::MAX) + 1,
        }
    }

    fn evaluate_offset(
        &mut self,
        data: FontData<'_>,
        offset: u32,
        depth: usize,
    ) -> Result<bool, ConditionError<E>> {
        if offset == 0 {
            return Ok(true);
        }
        let data = data
            .split_off(offset as usize)
            .ok_or(ConditionError::Read(ReadError::OutOfBounds))?;
        let condition = Condition::read(data).map_err(ConditionError::Read)?;
        self.evaluate(&condition, depth)
    }

    fn evaluate(
        &mut self,
        condition: &Condition<'_>,
        depth: usize,
    ) -> Result<bool, ConditionError<E>> {
        if depth == 0 || self.remaining_ops == 0 {
            return Err(ConditionError::LimitExceeded);
        }
        self.remaining_ops -= 1;
        Ok(match condition {
            Condition::Format1AxisRange(condition) => {
                let coord = self
                    .coords
                    .get(usize::from(condition.axis_index()))
                    .copied()
                    .unwrap_or_default();
                coord >= condition.filter_range_min_value()
                    && coord <= condition.filter_range_max_value()
            }
            Condition::Format2VariableValue(condition) => {
                let index = condition.var_index();
                let delta = if index == NO_VARIATION_INDEX {
                    0.0
                } else {
                    (self.delta)(index).map_err(ConditionError::Delta)?
                };
                f64::from(condition.default_value()) + delta > 0.0
            }
            Condition::Format3And(condition) => {
                let offsets = condition.condition_offsets();
                if offsets.len() != usize::from(condition.condition_count()) {
                    return Err(ConditionError::Read(ReadError::OutOfBounds));
                }
                for offset in offsets {
                    if !self.evaluate_offset(
                        condition.offset_data(),
                        offset.get().to_u32(),
                        depth - 1,
                    )? {
                        return Ok(false);
                    }
                }
                true
            }
            Condition::Format4Or(condition) => {
                let offsets = condition.condition_offsets();
                if offsets.len() != usize::from(condition.condition_count()) {
                    return Err(ConditionError::Read(ReadError::OutOfBounds));
                }
                for offset in offsets {
                    if self.evaluate_offset(
                        condition.offset_data(),
                        offset.get().to_u32(),
                        depth - 1,
                    )? {
                        return Ok(true);
                    }
                }
                false
            }
            Condition::Format5Negate(condition) => !self.evaluate_offset(
                condition.offset_data(),
                condition.condition_offset().to_u32(),
                depth - 1,
            )?,
        })
    }
}
