use std::hash::Hasher;

use font_types::F2Dot14;

#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct Triple {
    pub(crate) minimum: f32,
    pub(crate) middle: f32,
    pub(crate) maximum: f32,
}

impl Triple {
    pub(crate) fn new(minimum: f32, middle: f32, maximum: f32) -> Self {
        Self {
            minimum,
            middle,
            maximum,
        }
    }

    pub(crate) fn point(p: f32) -> Self {
        Self::new(p, p, p)
    }

    #[allow(dead_code)]
    pub(crate) fn is_point(&self) -> bool {
        self.minimum == self.middle && self.middle == self.maximum
    }

    #[allow(dead_code)]
    pub(crate) fn contains(&self, value: f32) -> bool {
        self.minimum <= value && value <= self.maximum
    }

    pub(crate) fn reverse_negate(&self) -> Self {
        Self {
            minimum: -self.maximum,
            middle: -self.middle,
            maximum: -self.minimum,
        }
    }
}

impl std::hash::Hash for Triple {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.minimum.to_bits().hash(state);
        self.middle.to_bits().hash(state);
        self.maximum.to_bits().hash(state);
    }
}

impl Eq for Triple {}

impl PartialEq for Triple {
    fn eq(&self, other: &Self) -> bool {
        F2Dot14::from_f32(self.minimum) == F2Dot14::from_f32(other.minimum)
            && F2Dot14::from_f32(self.middle) == F2Dot14::from_f32(other.middle)
            && F2Dot14::from_f32(self.maximum) == F2Dot14::from_f32(other.maximum)
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub(crate) struct TripleDistances {
    pub(crate) negative: f32,
    pub(crate) positive: f32,
}

impl TripleDistances {
    pub(crate) fn new(negative: f32, positive: f32) -> Self {
        Self { negative, positive }
    }
}

impl From<Triple> for TripleDistances {
    fn from(triple: Triple) -> Self {
        TripleDistances {
            negative: triple.middle - triple.minimum,
            positive: triple.maximum - triple.middle,
        }
    }
}

type RebaseTentResultItem = (f32, Triple);
type RebaseTentResult = Vec<RebaseTentResultItem>;

const EPSILON: f32 = 1.0 / (1 << 14) as f32;
const MAX_F2DOT14: f32 = 0x7FFF as f32 / (1 << 14) as f32;

/// Evaluates a support scalar for a coordinate within a tent.
/// Copied from VarRegionAxis::evaluate()
fn support_scalar(coord: f32, tent: Triple) -> f32 {
    let start = tent.minimum;
    let peak = tent.middle;
    let end = tent.maximum;

    if start > peak || peak > end {
        return 1.0;
    }
    if start < 0.0 && end > 0.0 && peak != 0.0 {
        return 1.0;
    }

    if peak == 0.0 || coord == peak {
        return 1.0;
    }

    if coord <= start || end <= coord {
        return 0.0;
    }

    // Interpolate
    if coord < peak {
        (coord - start) / (peak - start)
    } else {
        (end - coord) / (end - peak)
    }
}

/// Renormalize a normalized value v to the range of an axis,
/// considering the prenormalized distances as well as the new axis limits.
///
/// Ported from fonttools
pub(crate) fn renormalize_value(
    v: f32,
    triple: Triple,
    triple_distances: TripleDistances,
    extrapolate: bool,
) -> f32 {
    let lower = triple.minimum;
    let def = triple.middle;
    let upper = triple.maximum;
    debug_assert!(lower <= def && def <= upper);

    let v = if extrapolate {
        v
    } else {
        v.clamp(lower, upper)
    };

    if v == def {
        return 0.0;
    }

    if def < 0.0 {
        return -renormalize_value(
            -v,
            triple.reverse_negate(),
            TripleDistances {
                positive: triple_distances.negative,
                negative: triple_distances.positive,
            },
            extrapolate,
        );
    }

    // default >= 0 and v != default
    if v > def {
        return (v - def) / (upper - def);
    }

    // v < def
    if lower >= 0.0 {
        return (v - def) / (def - lower);
    }

    // lower < 0 and v < default
    let total_distance = triple_distances.negative * (-lower) + triple_distances.positive * def;

    let v_distance = if v >= 0.0 {
        (def - v) * triple_distances.positive
    } else {
        (-v) * triple_distances.negative + triple_distances.positive * def
    };

    -v_distance / total_distance
}

/// Internal solving function that processes one side of the axis transformation.
fn solve(tent: Triple, axis_limit: Triple, negative: bool) -> RebaseTentResult {
    let mut out = Vec::new();

    let axis_min = axis_limit.minimum;
    let axis_def = axis_limit.middle;
    let axis_max = axis_limit.maximum;
    let mut lower = tent.minimum;
    let peak = tent.middle;
    let mut upper = tent.maximum;

    // Mirror the problem such that axis_def <= peak
    if axis_def > peak {
        let mut mirrored = solve(
            tent.reverse_negate(),
            axis_limit.reverse_negate(),
            !negative,
        );
        for item in &mut mirrored {
            item.1 = item.1.reverse_negate();
        }
        return mirrored;
    }
    // axis_def <= peak

    // Case 1: The whole deltaset falls outside the new limit
    if axis_max <= lower && axis_max < peak {
        return out; // No overlap
    }

    // Case 2: Partial overlap
    if axis_max < peak {
        let mult = support_scalar(axis_max, tent);
        let new_tent = Triple {
            minimum: lower,
            middle: axis_max,
            maximum: axis_max,
        };

        let mut sub_out = solve(new_tent, axis_limit, negative);
        for item in &mut sub_out {
            item.0 *= mult;
        }
        return sub_out;
    }

    // lower <= axis_def <= peak <= axis_max

    let gain = support_scalar(axis_def, tent);
    out.push((gain, Triple::default()));

    // Positive side
    let out_gain = support_scalar(axis_max, tent);

    // Case 3a/gain >= out_gain
    if gain >= out_gain {
        let crossing = peak + (1.0 - gain) * (upper - peak);
        let loc = Triple {
            minimum: lower.max(axis_def),
            middle: peak,
            maximum: crossing,
        };

        out.push((1.0 - gain, loc));

        // The part after the crossing point
        if upper >= axis_max {
            let loc = Triple {
                minimum: crossing,
                middle: axis_max,
                maximum: axis_max,
            };
            out.push((out_gain - gain, loc));
        } else {
            // A tent's peak cannot fall on axis default. Nudge it.
            if upper == axis_def {
                upper += EPSILON;
            }

            // Downslope
            let loc1 = Triple {
                minimum: crossing,
                middle: upper,
                maximum: axis_max,
            };
            out.push((0.0 - gain, loc1));

            // Eternity justify
            let loc2 = Triple {
                minimum: upper,
                middle: axis_max,
                maximum: axis_max,
            };
            out.push((0.0 - gain, loc2));
        }
    } else {
        // Special-case if peak is at axis_max
        if axis_max == peak {
            upper = peak;
        }

        // Case 3: Scale the axis upper to achieve new tent
        let new_upper = peak + (1.0 - gain) * (upper - peak);
        debug_assert!(axis_max <= new_upper); // Because out_gain > gain

        // Note: The original C++ code has this disabled due to OTS compatibility
        // Keeping it disabled here as well
        #[allow(clippy::overly_complex_bool_expr)]
        if false && (new_upper <= axis_def + (axis_max - axis_def) * 2.0) {
            upper = new_upper;
            if !negative && axis_def + (axis_max - axis_def) * MAX_F2DOT14 < upper {
                upper = axis_def + (axis_max - axis_def) * MAX_F2DOT14;
                debug_assert!(peak < upper);
            }

            let loc = Triple {
                minimum: axis_def.max(lower),
                middle: peak,
                maximum: upper,
            };
            out.push((1.0 - gain, loc));
        } else {
            // Case 4: Chop into two tents
            let loc1 = Triple {
                minimum: axis_def.max(lower),
                middle: peak,
                maximum: axis_max,
            };
            out.push((1.0 - gain, loc1));

            let loc2 = Triple {
                minimum: peak,
                middle: axis_max,
                maximum: axis_max,
            };

            // Don't add a dirac delta!
            if peak < axis_max {
                out.push((out_gain - gain, loc2));
            }
        }
    }

    // Negative side

    // Case 1neg: Lower extends beyond axis_min
    if lower <= axis_min {
        let scalar = support_scalar(axis_min, tent);
        let loc = Triple {
            minimum: axis_min,
            middle: axis_min,
            maximum: axis_def,
        };
        out.push((scalar - gain, loc));
    } else {
        // Case 2neg: Lower is between axis_min and axis_def
        // A tent's peak cannot fall on axis default. Nudge it.
        if lower == axis_def {
            lower -= EPSILON;
        }

        // Downslope
        let loc1 = Triple {
            minimum: axis_min,
            middle: lower,
            maximum: axis_def,
        };
        out.push((0.0 - gain, loc1));

        // Eternity justify
        let loc2 = Triple {
            minimum: axis_min,
            middle: axis_min,
            maximum: lower,
        };
        out.push((0.0 - gain, loc2));
    }

    out
}

/* Given a tuple (lower,peak,upper) "tent" and new axis limits
 * (axisMin,axisDefault,axisMax), solves how to represent the tent
 * under the new axis configuration.  All values are in normalized
 * -1,0,+1 coordinate system. Tent values can be outside this range.
 *
 * Return value: a list of tuples. Each tuple is of the form
 * (scalar,tent), where scalar is a multipler to multiply any
 * delta-sets by, and tent is a new tent for that output delta-set.
 * If tent value is Triple{}, that is a special deltaset that should
 * be always-enabled (called "gain").
 */
pub(crate) fn rebase_tent(
    tent: Triple,
    axis_limit: Triple,
    axis_triple_distances: TripleDistances,
) -> RebaseTentResult {
    debug_assert!(-1.0 <= axis_limit.minimum && axis_limit.minimum <= axis_limit.middle);
    debug_assert!(axis_limit.middle <= axis_limit.maximum && axis_limit.maximum <= 1.0);
    debug_assert!(-2.0 <= tent.minimum && tent.minimum <= tent.middle);
    debug_assert!(tent.middle <= tent.maximum && tent.maximum <= 2.0);
    debug_assert!(tent.middle.abs() >= f32::EPSILON, "tent middle was zero",);

    let sols = solve(tent, axis_limit, false);

    let mut out = Vec::new();
    for (scalar, sol_tent) in sols {
        if scalar == 0.0 {
            continue;
        }
        if sol_tent == Triple::default() {
            out.push((scalar, sol_tent));
            continue;
        }

        let normalized = Triple {
            minimum: renormalize_value(sol_tent.minimum, axis_limit, axis_triple_distances, false),
            middle: renormalize_value(sol_tent.middle, axis_limit, axis_triple_distances, false),
            maximum: renormalize_value(sol_tent.maximum, axis_limit, axis_triple_distances, false),
        };
        out.push((scalar, normalized));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.000001
    }

    fn approx_triple(a: Triple, b: Triple) -> bool {
        approx(a.minimum, b.minimum) && approx(a.middle, b.middle) && approx(a.maximum, b.maximum)
    }

    #[test]
    fn test_case_1_pin_axis_0() {
        let tent = Triple::new(0.0, 1.0, 1.0);
        let axis_range = Triple::new(0.0, 0.0, 0.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn test_case_1_pin_axis_05() {
        let tent = Triple::new(0.0, 1.0, 1.0);
        let axis_range = Triple::new(0.5, 0.5, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 0.5));
        assert_eq!(out[0].1, Triple::default());
    }

    #[test]
    fn test_case_1_tent_outside() {
        let tent = Triple::new(0.3, 0.5, 0.8);
        let axis_range = Triple::new(0.1, 0.2, 0.3);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn test_case_2_tent_0_1_1_axis_neg1_0_05() {
        let tent = Triple::new(0.0, 1.0, 1.0);
        let axis_range = Triple::new(-1.0, 0.0, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 0.5));
        assert_eq!(out[0].1, Triple::new(0.0, 1.0, 1.0));
    }

    #[test]
    fn test_case_2_tent_0_1_1_axis_neg1_0_075() {
        let tent = Triple::new(0.0, 1.0, 1.0);
        let axis_range = Triple::new(-1.0, 0.0, 0.75);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 0.75));
        assert_eq!(out[0].1, Triple::new(0.0, 1.0, 1.0));
    }

    #[test]
    fn test_case_3_no_gain() {
        let tent = Triple::new(0.0, 0.2, 1.0);
        let axis_range = Triple::new(-1.0, 0.0, 0.8);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(0.0, 0.25, 1.0));
        assert!(approx(out[1].0, 0.250));
        assert_triple_approx(&out[1].1, &Triple::new(0.25, 1.0, 1.0));
    }

    #[test]
    fn test_case_3_boundary() {
        let tent = Triple::new(0.0, 0.4, 1.0);
        let axis_range = Triple::new(-1.0, 0.0, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(0.0, 0.8, 1.0));
        assert!(approx(out[1].0, 2.5 / 3.0));
        assert_triple_approx(&out[1].1, &Triple::new(0.8, 1.0, 1.0));
    }

    #[test]
    fn test_case_4_tent_0_025_1() {
        let tent = Triple::new(0.0, 0.25, 1.0);
        let axis_range = Triple::new(-1.0, 0.0, 0.4);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(0.0, 0.625, 1.0));
        assert!(approx(out[1].0, 0.80));
        assert_triple_approx(&out[1].1, &Triple::new(0.625, 1.0, 1.0));
    }

    #[test]
    fn test_case_4_tent_025_03_105() {
        let tent = Triple::new(0.25, 0.3, 1.05);
        let axis_range = Triple::new(0.0, 0.2, 0.4);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(0.25, 0.5, 1.0));
        assert!(approx(out[1].0, 2.6 / 3.0));
        assert_triple_approx(&out[1].1, &Triple::new(0.5, 1.0, 1.0));
    }

    #[test]
    fn test_case_4_boundary() {
        let tent = Triple::new(0.25, 0.5, 1.0);
        let axis_range = Triple::new(0.0, 0.25, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::new(0.0, 1.0, 1.0));
    }

    #[test]
    fn test_case_3a_1neg_1() {
        let tent = Triple::new(0.0, 0.5, 1.0);
        let axis_range = Triple::new(0.0, 0.5, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 3);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -1.0));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
        assert!(approx(out[2].0, -1.0));
        assert_triple_approx(&out[2].1, &Triple::new(-1.0, -1.0, 0.0));
    }

    #[test]
    fn test_case_3a_1neg_2() {
        let tent = Triple::new(0.0, 0.5, 1.0);
        let axis_range = Triple::new(0.0, 0.5, 0.75);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 3);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -0.5));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
        assert!(approx(out[2].0, -1.0));
        assert_triple_approx(&out[2].1, &Triple::new(-1.0, -1.0, 0.0));
    }

    #[test]
    fn test_complex_case_1() {
        let tent = Triple::new(0.0, 0.50, 1.0);
        let axis_range = Triple::new(0.0, 0.25, 0.8);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 4);
        assert!(approx(out[0].0, 0.5));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, 0.5));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 0.454545, 0.909091));
        assert!(approx(out[2].0, -0.1));
        assert_triple_approx(&out[2].1, &Triple::new(0.909091, 1.0, 1.0));
        assert!(approx(out[3].0, -0.5));
        assert_triple_approx(&out[3].1, &Triple::new(-1.0, -1.0, 0.0));
    }

    #[test]
    fn test_case_3a_1neg_3() {
        let tent = Triple::new(0.0, 0.5, 2.0);
        let axis_range = Triple::new(0.2, 0.5, 0.8);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 3);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -0.2));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
        assert!(approx(out[2].0, -0.6));
        assert_triple_approx(&out[2].1, &Triple::new(-1.0, -1.0, 0.0));
    }

    #[test]
    fn test_case_3a_1neg_4() {
        let tent = Triple::new(0.0, 0.5, 2.0);
        let axis_range = Triple::new(0.2, 0.5, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 3);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -1.0 / 3.0));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
        assert!(approx(out[2].0, -0.6));
        assert_triple_approx(&out[2].1, &Triple::new(-1.0, -1.0, 0.0));
    }

    #[test]
    fn test_case_3_with_different_axis_def() {
        let tent = Triple::new(0.0, 0.5, 1.0);
        let axis_range = Triple::new(0.25, 0.25, 0.75);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 0.5));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, 0.5));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 0.5, 1.0));
    }

    #[test]
    fn test_case_1neg() {
        let tent = Triple::new(0.0, 0.5, 1.0);
        let axis_range = Triple::new(0.0, 0.25, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 3);
        assert!(approx(out[0].0, 0.5));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, 0.5));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
        assert!(approx(out[2].0, -0.5));
        assert_triple_approx(&out[2].1, &Triple::new(-1.0, -1.0, 0.0));
    }

    #[test]
    fn test_case_2neg() {
        let tent = Triple::new(0.05, 0.55, 1.0);
        let axis_range = Triple::new(0.0, 0.25, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 4);
        assert!(approx(out[0].0, 0.4));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, 0.5));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
        assert!(approx(out[2].0, -0.4));
        assert_triple_approx(&out[2].1, &Triple::new(-1.0, -0.8, 0.0));
        assert!(approx(out[3].0, -0.4));
        assert_triple_approx(&out[3].1, &Triple::new(-1.0, -1.0, -0.8));
    }

    #[test]
    fn test_case_2neg_other_side() {
        let tent = Triple::new(-1.0, -0.55, -0.05);
        let axis_range = Triple::new(-0.5, -0.25, 0.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 4);
        assert!(approx(out[0].0, 0.4));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, 0.5));
        assert_triple_approx(&out[1].1, &Triple::new(-1.0, -1.0, 0.0));
        assert!(approx(out[2].0, -0.4));
        assert_triple_approx(&out[2].1, &Triple::new(0.0, 0.8, 1.0));
        assert!(approx(out[3].0, -0.4));
        assert_triple_approx(&out[3].1, &Triple::new(0.8, 1.0, 1.0));
    }

    #[test]
    fn test_corner_case_point_0() {
        let tent = Triple::new(0.5, 0.5, 0.5);
        let axis_range = Triple::new(0.5, 0.5, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
    }

    #[test]
    fn test_corner_case_complex() {
        let tent = Triple::new(0.3, 0.5, 0.7);
        let axis_range = Triple::new(0.1, 0.5, 0.9);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 5);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -1.0));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 0.5, 1.0));
        assert!(approx(out[2].0, -1.0));
        assert_triple_approx(&out[2].1, &Triple::new(0.5, 1.0, 1.0));
        assert!(approx(out[3].0, -1.0));
        assert_triple_approx(&out[3].1, &Triple::new(-1.0, -0.5, 0.0));
        assert!(approx(out[4].0, -1.0));
        assert_triple_approx(&out[4].1, &Triple::new(-1.0, -1.0, -0.5));
    }

    #[test]
    fn test_point_in_range_0() {
        let tent = Triple::new(0.5, 0.5, 0.5);
        let axis_range = Triple::new(0.25, 0.25, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::new(1.0, 1.0, 1.0));
    }

    #[test]
    fn test_point_in_range_1() {
        let tent = Triple::new(0.5, 0.5, 0.5);
        let axis_range = Triple::new(0.25, 0.35, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::new(1.0, 1.0, 1.0));
    }

    #[test]
    fn test_point_near_range() {
        let tent = Triple::new(0.5, 0.5, 0.55);
        let axis_range = Triple::new(0.25, 0.35, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::new(1.0, 1.0, 1.0));
    }

    #[test]
    fn test_point_at_boundary() {
        let tent = Triple::new(0.5, 0.5, 1.0);
        let axis_range = Triple::new(0.5, 0.5, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -1.0));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
    }

    #[test]
    fn test_peak_before_boundary() {
        let tent = Triple::new(0.25, 0.5, 1.0);
        let axis_range = Triple::new(0.5, 0.5, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -1.0));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0, 1.0));
    }

    #[test]
    fn test_peak_at_zero() {
        let tent = Triple::new(0.0, 0.2, 1.0);
        let axis_range = Triple::new(0.0, 0.0, 0.5);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 2);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(0.0, 0.4, 1.0));
        assert!(approx(out[1].0, 0.625));
        assert_triple_approx(&out[1].1, &Triple::new(0.4, 1.0, 1.0));
    }

    #[test]
    fn test_wide_axis_range() {
        let tent = Triple::new(0.0, 0.5, 1.0);
        let axis_range = Triple::new(-1.0, 0.25, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 5);
        assert!(approx(out[0].0, 0.5));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, 0.5));
        assert_triple_approx(&out[1].1, &Triple::new(0.0, 1.0 / 3.0, 2.0 / 3.0));
        assert!(approx(out[2].0, -0.5));
        assert_triple_approx(&out[2].1, &Triple::new(2.0 / 3.0, 1.0, 1.0));
        assert!(approx(out[3].0, -0.5));
        assert_triple_approx(&out[3].1, &Triple::new(-1.0, -0.2, 0.0));
        assert!(approx(out[4].0, -0.5));
        assert_triple_approx(&out[4].1, &Triple::new(-1.0, -1.0, -0.2));
    }

    #[test]
    fn test_point_axis_center() {
        let tent = Triple::new(0.5, 0.5, 0.5);
        let axis_range = Triple::new(0.0, 0.5, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 5);
        assert!(approx(out[0].0, 1.0));
        assert_eq!(out[0].1, Triple::default());
        assert!(approx(out[1].0, -1.0));
        let epsilon = 1.0 / (1 << 14) as f32;
        assert_triple_approx(&out[1].1, &Triple::new(0.0, epsilon * 2.0, 1.0));
        assert!(approx(out[2].0, -1.0));
        assert_triple_approx(&out[2].1, &Triple::new(epsilon * 2.0, 1.0, 1.0));
        assert!(approx(out[3].0, -1.0));
        assert_triple_approx(&out[3].1, &Triple::new(-1.0, -epsilon * 2.0, 0.0));
        assert!(approx(out[4].0, -1.0));
        assert_triple_approx(&out[4].1, &Triple::new(-1.0, -1.0, -epsilon * 2.0));
    }

    #[test]
    fn test_axis_default_negative() {
        let tent = Triple::new(0.0, 1.0, 1.0);
        let axis_range = Triple::new(-1.0, -0.5, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(1.0 / 3.0, 1.0, 1.0));
    }

    #[test]
    fn test_axis_distances_asymmetric() {
        let tent = Triple::new(0.0, 1.0, 1.0);
        let axis_range = Triple::new(-1.0, -0.5, 1.0);
        let axis_distances = TripleDistances {
            negative: 2.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(0.5, 1.0, 1.0));
    }

    #[test]
    fn test_renormalize_with_asymmetric_distances() {
        let tent = Triple::new(0.6, 0.7, 0.8);
        let axis_range = Triple::new(-1.0, 0.2, 1.0);
        let axis_distances = TripleDistances {
            negative: 1.0,
            positive: 1.0,
        };
        let out = rebase_tent(tent, axis_range, axis_distances);
        assert_eq!(out.len(), 1);
        assert!(approx(out[0].0, 1.0));
        assert_triple_approx(&out[0].1, &Triple::new(0.5, 0.625, 0.75));
    }

    fn assert_triple_approx(a: &Triple, b: &Triple) {
        assert!(
            approx_triple(*a, *b),
            "Triples not approximately equal: {:?} vs {:?}",
            a,
            b
        );
    }
}
