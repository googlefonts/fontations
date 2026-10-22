//! Rebase blend expressions, including variable defaults and variable deltas.
use super::{charstring::Value, Error, Result};
use crate::instance::StorePlan;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Rounding {
    compensated: bool,
    exact: f64,
    emitted: f64,
}
impl Rounding {
    pub fn private_dict() -> Self {
        Self {
            compensated: true,
            ..Self::default()
        }
    }
    fn fold(&mut self, value: f64) -> f64 {
        if !self.compensated {
            return value.round();
        }
        // Private DICT values are commonly delta-encoded. Bound accumulated
        // error in their decoded absolute values, matching HarfBuzz's policy.
        self.exact += value;
        let emitted = self.exact.round();
        let delta = emitted - self.emitted;
        self.emitted = emitted;
        delta
    }
}

pub(super) struct Rebaser<'a> {
    pub store: &'a StorePlan,
    pub gains: &'a [Vec<f64>],
    pub constants: &'a BTreeMap<usize, usize>,
    remaining: usize,
}
impl<'a> Rebaser<'a> {
    pub fn new(
        store: &'a StorePlan,
        gains: &'a [Vec<f64>],
        constants: &'a BTreeMap<usize, usize>,
    ) -> Self {
        Self {
            store,
            gains,
            constants,
            remaining: 200_000,
        }
    }
    pub fn value(&mut self, value: &Value, rounding: &mut Rounding) -> Result<Value> {
        self.remaining = self.remaining.checked_sub(1).ok_or(Error)?;
        let Some(blend) = &value.blend else {
            return Ok(value.expression());
        };
        let base = self.value(&blend.base, rounding)?;
        let deltas = blend
            .deltas
            .iter()
            .map(|v| self.value(v, rounding))
            .collect::<Result<Vec<_>>>()?;
        let gains = self.gains.get(blend.ivs).ok_or(Error)?;
        let transform = self.store.transforms.get(blend.ivs).ok_or(Error)?;
        let constant = self.constants.get(&blend.ivs).copied();
        // A region scalar becomes a constant gain plus rebased region terms.
        // Keep deltas as expressions so this substitution preserves products
        // of variable values as well as ordinary linear blends.
        let folded = self.combination(&deltas, gains, blend.ivs, constant, Some(rounding))?;
        let base = self.add(&base, &folded, blend.ivs, constant)?;
        let residual = transform
            .weights
            .iter()
            .map(|weights| self.combination(&deltas, weights, blend.ivs, constant, None))
            .collect::<Result<Vec<_>>>()?;
        if residual
            .iter()
            .all(|v| v.blend.is_none() && v.default == 0.)
        {
            return Ok(base);
        }
        self.node(&base, &residual, blend.ivs)
    }
    fn node(&mut self, base: &Value, deltas: &[Value], ivs: usize) -> Result<Value> {
        self.remaining = self.remaining.checked_sub(1 + deltas.len()).ok_or(Error)?;
        Value::blended(base, deltas, ivs)
    }
    fn scale(&mut self, value: &Value, factor: f64) -> Result<Value> {
        if factor == 0. {
            return Ok(Value::plain(0.));
        }
        if factor == 1. {
            return Ok(value.expression());
        }
        if let Some(blend) = &value.blend {
            let base = self.scale(&blend.base, factor)?;
            let deltas = blend
                .deltas
                .iter()
                .map(|v| self.scale(v, factor))
                .collect::<Result<Vec<_>>>()?;
            self.node(&base, &deltas, blend.ivs)
        } else {
            Ok(Value::plain(value.default * factor))
        }
    }
    fn add_number(&mut self, value: &Value, number: f64) -> Result<Value> {
        if number == 0. {
            return Ok(value.expression());
        }
        if let Some(blend) = &value.blend {
            let base = self.add_number(&blend.base, number)?;
            self.node(&base, &blend.deltas, blend.ivs)
        } else {
            Ok(Value::plain(value.default + number))
        }
    }
    fn add(&mut self, a: &Value, b: &Value, ivs: usize, constant: Option<usize>) -> Result<Value> {
        if b.blend.is_none() {
            return self.add_number(a, b.default);
        }
        if a.blend.is_none() {
            return self.add_number(b, a.default);
        }
        let slot = constant.ok_or(Error)?;
        let count = self.store.transforms.get(ivs).ok_or(Error)?.indices.len();
        let mut deltas = vec![Value::plain(0.); count];
        // A zero-peak region has scalar 1 at every location, so this blend
        // represents a + b while retaining the glyph's sole active vsindex.
        *deltas.get_mut(slot).ok_or(Error)? = b.expression();
        self.node(a, &deltas, ivs)
    }
    fn combination(
        &mut self,
        values: &[Value],
        weights: &[f64],
        ivs: usize,
        constant: Option<usize>,
        rounding: Option<&mut Rounding>,
    ) -> Result<Value> {
        if values.len() != weights.len() {
            return Err(Error);
        }
        if values
            .iter()
            .zip(weights)
            .all(|(v, w)| *w == 0. || v.blend.is_none())
        {
            let sum = values
                .iter()
                .zip(weights)
                .map(|(v, w)| v.default * w)
                .sum::<f64>();
            return Ok(Value::plain(
                rounding.map_or_else(|| sum.round(), |r| r.fold(sum)),
            ));
        }
        let mut result = None;
        // Preserve fractional coefficients of variable terms: independently
        // rounding their leaves would change a higher-order expression.
        for (value, &weight) in values.iter().zip(weights).filter(|(_, w)| **w != 0.) {
            let value = self.scale(value, weight)?;
            result = Some(if let Some(old) = result {
                self.add(&old, &value, ivs, constant)?
            } else {
                value
            });
        }
        result.ok_or(Error)
    }
}
