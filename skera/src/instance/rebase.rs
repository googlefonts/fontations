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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Triple(pub f64, pub f64, pub f64);
impl Triple {
    fn reverse(self) -> Self {
        Self(-self.2, -self.1, -self.0)
    }
}
pub(super) fn scalar(v: f64, t: Triple) -> f64 {
    let Triple(start, peak, end) = t;
    if start > peak
        || peak > end
        || (start < 0. && end > 0. && peak != 0.)
        || peak == 0.
        || v == peak
    {
        1.
    } else if v <= start || v >= end {
        0.
    } else if v < peak {
        (v - start) / (peak - start)
    } else {
        (end - v) / (end - peak)
    }
}
pub(super) fn renormalize(v: f64, t: Triple, dist: (f64, f64)) -> f64 {
    let Triple(min, def, max) = t;
    if v == def {
        return 0.;
    }
    if def < 0. {
        return -renormalize(-v, t.reverse(), (dist.1, dist.0));
    }
    if v > def {
        return if max == def {
            1.
        } else {
            (v - def) / (max - def)
        };
    }
    if min >= 0. {
        return if min == def {
            -1.
        } else {
            (v - def) / (def - min)
        };
    }
    let total = dist.0 * (-min) + dist.1 * def;
    if total == 0. {
        return 0.;
    }
    -(if v >= 0. {
        (def - v) * dist.1
    } else {
        (-v) * dist.0 + dist.1 * def
    }) / total
}
fn solve(tent: Triple, limit: Triple) -> Vec<(f64, Option<Triple>)> {
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
        let crossing = peak + (1. - gain) * (upper - peak);
        out.push((1. - gain, Some(Triple(lower.max(def), peak, crossing))));
        if upper >= max {
            out.push((out_gain - gain, Some(Triple(crossing, max, max))));
        } else {
            if upper == def {
                upper += 1. / 16384.;
            }
            out.push((-gain, Some(Triple(crossing, upper, max))));
            out.push((-gain, Some(Triple(upper, max, max))));
        }
    } else {
        out.push((1. - gain, Some(Triple(lower.max(def), peak, max))));
        if peak < max {
            out.push((out_gain - gain, Some(Triple(peak, max, max))));
        }
    }
    if lower <= min {
        out.push((scalar(min, tent) - gain, Some(Triple(min, min, def))));
    } else {
        if lower == def {
            lower -= 1. / 16384.;
        }
        out.push((-gain, Some(Triple(min, lower, def))));
        out.push((-gain, Some(Triple(min, min, lower))));
    }
    out
}
pub(super) fn rebase(tent: Triple, limit: Triple, dist: (f64, f64)) -> Vec<(f64, Option<Triple>)> {
    // Invalid and neutral axes have no influence on a region's scalar.
    if tent.0 > tent.1 || tent.1 > tent.2 || tent.1 == 0. || (tent.0 < 0. && tent.2 > 0.) {
        return vec![(1., None)];
    }
    if limit.0 == limit.2 {
        let gain = scalar(limit.1, tent);
        return if gain == 0. {
            vec![]
        } else {
            vec![(gain, None)]
        };
    }
    solve(tent, limit)
        .into_iter()
        .filter(|(g, _)| *g != 0.)
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
