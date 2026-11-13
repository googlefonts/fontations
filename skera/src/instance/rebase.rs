// Copyright © 2023 Behdad Esfahbod
//
// Permission is hereby granted, without written agreement and without
// license or royalty fees, to use, copy, modify, and distribute this
// software and its documentation for any purpose, provided that the
// above copyright notice and the following two paragraphs appear in
// all copies of this software.
//
// IN NO EVENT SHALL THE COPYRIGHT HOLDER BE LIABLE TO ANY PARTY FOR
// DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES
// ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS DOCUMENTATION, EVEN
// IF THE COPYRIGHT HOLDER HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
//
// THE COPYRIGHT HOLDER SPECIFICALLY DISCLAIMS ANY WARRANTIES, INCLUDING,
// BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
// FITNESS FOR A PARTICULAR PURPOSE. THE SOFTWARE PROVIDED HEREUNDER IS
// ON AN "AS IS" BASIS, AND THE COPYRIGHT HOLDER HAS NO OBLIGATION TO
// PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
//
// Ported from HarfBuzz's hb-subset-instancer-solver.cc, itself a port of
// fontTools.varLib.instancer.solver. Keep the OTS-compatible split-tent policy.

use std::ops::{Add, Div, Mul, Neg, Sub};

// Keep one solver while allowing each consumer to retain its arithmetic policy.
pub(super) trait Scalar:
    Copy
    + From<f32>
    + PartialOrd
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    fn max(self, other: Self) -> Self;
}
impl Scalar for f32 {
    fn max(self, other: Self) -> Self {
        self.max(other)
    }
}
impl Scalar for f64 {
    fn max(self, other: Self) -> Self {
        self.max(other)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Triple<T = f64>(pub T, pub T, pub T);
impl<T> Triple<T> {
    pub(super) fn map<U>(self, f: impl Fn(T) -> U) -> Triple<U> {
        Triple(f(self.0), f(self.1), f(self.2))
    }
    fn reverse(self) -> Self
    where
        T: Neg<Output = T>,
    {
        Self(-self.2, -self.1, -self.0)
    }
}
pub(super) fn scalar<T: Scalar>(v: T, t: Triple<T>) -> T {
    let zero = T::from(0.);
    let one = T::from(1.);
    let Triple(start, peak, end) = t;
    if start > peak
        || peak > end
        || (start < zero && end > zero && peak != zero)
        || peak == zero
        || v == peak
    {
        one
    } else if v <= start || v >= end {
        zero
    } else if v < peak {
        (v - start) / (peak - start)
    } else {
        (end - v) / (end - peak)
    }
}
pub(super) fn renormalize<T: Scalar>(v: T, t: Triple<T>, dist: (T, T)) -> T {
    let zero = T::from(0.);
    let one = T::from(1.);
    let Triple(min, def, max) = t;
    if v == def {
        return zero;
    }
    if def < zero {
        return -renormalize(-v, t.reverse(), (dist.1, dist.0));
    }
    if v > def {
        return if max == def {
            one
        } else {
            (v - def) / (max - def)
        };
    }
    if min >= zero {
        return if min == def {
            -one
        } else {
            (v - def) / (def - min)
        };
    }
    let total = dist.0 * (-min) + dist.1 * def;
    if total == zero {
        return zero;
    }
    -(if v >= zero {
        (def - v) * dist.1
    } else {
        (-v) * dist.0 + dist.1 * def
    }) / total
}
fn solve<T: Scalar>(tent: Triple<T>, limit: Triple<T>) -> Vec<(T, Option<Triple<T>>)> {
    let one = T::from(1.);
    let Triple(min, def, max) = limit;
    let Triple(mut lower, peak, mut upper) = tent;
    if def > peak {
        return solve(tent.reverse(), limit.reverse())
            .into_iter()
            .map(|(g, t)| (g, t.map(Triple::reverse)))
            .collect();
    }
    if max <= lower && max < peak {
        return vec![];
    }
    if max < peak {
        let g = scalar(max, tent);
        return solve(Triple(lower, max, max), limit)
            .into_iter()
            .map(|(s, t)| (s * g, t))
            .collect();
    }
    let gain = scalar(def, tent);
    let out_gain = scalar(max, tent);
    let mut out = vec![(gain, None)];
    if gain >= out_gain {
        let crossing = peak + (one - gain) * (upper - peak);
        out.push((one - gain, Some(Triple(lower.max(def), peak, crossing))));
        if upper >= max {
            out.push((out_gain - gain, Some(Triple(crossing, max, max))));
        } else {
            if upper == def {
                upper = upper + T::from(1. / 16384.);
            }
            out.push((-gain, Some(Triple(crossing, upper, max))));
            out.push((-gain, Some(Triple(upper, max, max))));
        }
    } else {
        out.push((one - gain, Some(Triple(lower.max(def), peak, max))));
        if peak < max {
            out.push((out_gain - gain, Some(Triple(peak, max, max))));
        }
    }
    if lower <= min {
        out.push((scalar(min, tent) - gain, Some(Triple(min, min, def))));
    } else {
        if lower == def {
            lower = lower - T::from(1. / 16384.);
        }
        out.push((-gain, Some(Triple(min, lower, def))));
        out.push((-gain, Some(Triple(min, min, lower))));
    }
    out
}
pub(super) fn rebase<T: Scalar>(
    tent: Triple<T>,
    limit: Triple<T>,
    dist: (T, T),
) -> Vec<(T, Option<Triple<T>>)> {
    let zero = T::from(0.);
    let one = T::from(1.);
    // Invalid and neutral axes have no influence on a region's scalar.
    if tent.0 > tent.1 || tent.1 > tent.2 || tent.1 == zero || (tent.0 < zero && tent.2 > zero) {
        return vec![(one, None)];
    }
    if limit.0 == limit.2 {
        let gain = scalar(limit.1, tent);
        return if gain == zero {
            vec![]
        } else {
            vec![(gain, None)]
        };
    }
    solve(tent, limit)
        .into_iter()
        .filter(|(g, _)| *g != zero)
        .map(|(g, t)| {
            (
                g,
                t.map(|t| {
                    Triple(
                        renormalize(t.0, limit, dist),
                        renormalize(t.1, limit, dist),
                        renormalize(t.2, limit, dist),
                    )
                }),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebased_tents_preserve_the_function() {
        for tent in [
            Triple(0., 0.5, 1.),
            Triple(0.2, 0.6, 0.9),
            Triple(-1., -0.5, 0.),
        ] {
            for limit in [
                Triple(-1., 0., 0.8),
                Triple(-0.8, 0.3, 0.9),
                Triple(-0.9, -0.2, 0.8),
                Triple(0.1, 0.4, 0.8),
            ] {
                let dist = (200., 500.);
                let terms = rebase(tent, limit, dist);
                for i in 0..101 {
                    let v =
                        (limit.0 + (limit.2 - limit.0) * i as f64 / 100.).clamp(limit.0, limit.2);
                    let n = renormalize(v, limit, dist);
                    let got = terms
                        .iter()
                        .map(|(g, t)| g * t.map_or(1., |t| scalar(n, t)))
                        .sum::<f64>();
                    assert!(
                        (got - scalar(v, tent)).abs() < 1e-10,
                        "{tent:?} {limit:?} {v} {got}"
                    );
                }
            }
        }
    }
}
