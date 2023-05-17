//! OpenType variations common table formats

include!("../../generated/generated_variations.rs");

use std::collections::HashSet;

use crate::util::WrappingGet;
use kurbo::{Point, Vec2};
pub use read_fonts::tables::variations::{TupleIndex, TupleVariationCount};

impl TupleVariationHeader {
    pub fn new(
        variation_data_size: u16,
        shared_tuple_idx: Option<u16>,
        peak_tuple: Option<Tuple>,
        intermediate_region: Option<(Tuple, Tuple)>,
        has_private_points: bool,
    ) -> Self {
        assert!(
            shared_tuple_idx.is_some() != peak_tuple.is_some(),
            "one and only one of peak_tuple or shared_tuple_idx must be present"
        );
        let mut idx = shared_tuple_idx.unwrap_or_default();
        if peak_tuple.is_some() {
            idx |= TupleIndex::EMBEDDED_PEAK_TUPLE;
        }
        if intermediate_region.is_some() {
            idx |= TupleIndex::INTERMEDIATE_REGION;
        }
        if has_private_points {
            idx |= TupleIndex::PRIVATE_POINT_NUMBERS;
        }
        let (intermediate_start_tuple, intermediate_end_tuple) = intermediate_region
            .map(|(start, end)| (start.values, end.values))
            .unwrap_or_default();

        TupleVariationHeader {
            variation_data_size,
            tuple_index: TupleIndex::from_bits(idx),
            peak_tuple: peak_tuple.map(|tup| tup.values).unwrap_or_default(),
            intermediate_start_tuple,
            intermediate_end_tuple,
        }
    }

    /// Return the number of bytes required to encode this header
    pub fn compute_size(&self) -> u16 {
        let len: usize = 2 + 2 // variationDataSize, tupleIndex
        + self.peak_tuple.len() * F2Dot14::RAW_BYTE_LEN
        + self.intermediate_start_tuple.len()  * F2Dot14::RAW_BYTE_LEN
        + self.intermediate_end_tuple.len()  * F2Dot14::RAW_BYTE_LEN;
        len.try_into().unwrap()
    }
}

impl VariationRegionList {
    fn compute_axis_count(&self) -> usize {
        let count = self
            .variation_regions
            .first()
            .map(|reg| reg.region_axes.len())
            .unwrap_or(0);
        //TODO: check this at validation time
        debug_assert!(self
            .variation_regions
            .iter()
            .map(|reg| reg.region_axes.len())
            .all(|n| n == count));
        count
    }
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-point-numbers>
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq)]
pub enum PackedPointNumbers {
    /// Contains deltas for all point numbers
    #[default]
    All,
    /// Contains deltas only for these specific point numbers
    Some(Vec<u16>),
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
#[derive(Clone, Debug, Default)]
pub struct PackedDeltas {
    deltas: Vec<i16>,
}

impl Validate for PackedDeltas {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}

impl FontWrite for PackedDeltas {
    fn write_into(&self, writer: &mut TableWriter) {
        for run in self.iter_runs() {
            run.write_into(writer)
        }
    }
}

impl PackedDeltas {
    /// Construct a `PackedDeltas` from a vector of raw delta values.
    pub fn new(deltas: Vec<i16>) -> Self {
        Self { deltas }
    }

    /// Compute the number of bytes required to encode these deltas
    pub(crate) fn compute_size(&self) -> u16 {
        self.iter_runs().fold(0u16, |acc, run| {
            acc.checked_add(run.compute_size()).unwrap()
        })
    }

    fn iter_runs(&self) -> impl Iterator<Item = PackedDeltaRun> {
        // 6 bits for length per https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas
        const MAX_POINTS_PER_RUN: usize = 64;

        fn in_i8_range(val: i16) -> bool {
            const MIN: i16 = i8::MIN as i16;
            const MAX: i16 = i8::MAX as i16;
            (MIN..=MAX).contains(&val)
        }

        fn count_leading_zeros(slice: &[i16]) -> u8 {
            slice
                .iter()
                .take(MAX_POINTS_PER_RUN)
                .take_while(|v| **v == 0)
                .count() as u8
        }

        /// compute the number of deltas in the next run, and whether they are i8s or not
        fn next_run_len(slice: &[i16]) -> (usize, bool) {
            let first = *slice.first().expect("bounds checked before here");
            debug_assert!(first != 0);
            let is_1_byte = in_i8_range(first);

            let mut idx = 1;
            while idx < MAX_POINTS_PER_RUN && idx < slice.len() {
                let cur = slice[idx];

                // Any reason to stop?
                let two_zeros = cur == 0 && slice.get(idx + 1) == Some(&0);
                let different_enc_len = in_i8_range(cur) != is_1_byte;
                if two_zeros || different_enc_len {
                    break;
                }

                idx += 1;
            }
            (idx, is_1_byte)
        }

        let mut deltas = self.deltas.as_slice();

        std::iter::from_fn(move || {
            if *deltas.first()? == 0 {
                let n_zeros = count_leading_zeros(deltas);
                deltas = &deltas[n_zeros as usize..];
                Some(PackedDeltaRun::Zeros(n_zeros))
            } else {
                let (len, is_i8) = next_run_len(deltas);
                let (head, tail) = deltas.split_at(len);
                deltas = tail;
                if is_i8 {
                    Some(PackedDeltaRun::OneByte(head))
                } else {
                    Some(PackedDeltaRun::TwoBytes(head))
                }
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PackedDeltaRun<'a> {
    Zeros(u8),
    OneByte(&'a [i16]),
    TwoBytes(&'a [i16]),
}

impl PackedDeltaRun<'_> {
    fn compute_flag(&self) -> u8 {
        /// Flag indicating that this run contains no data,
        /// and that the deltas for this run are all zero.
        const DELTAS_ARE_ZERO: u8 = 0x80;
        /// Flag indicating the data type for delta values in the run.
        const DELTAS_ARE_WORDS: u8 = 0x40;

        match self {
            PackedDeltaRun::Zeros(count) => (count - 1) | DELTAS_ARE_ZERO,
            PackedDeltaRun::OneByte(deltas) => deltas.len() as u8 - 1,
            PackedDeltaRun::TwoBytes(deltas) => (deltas.len() as u8 - 1) | DELTAS_ARE_WORDS,
        }
    }

    fn compute_size(&self) -> u16 {
        match self {
            PackedDeltaRun::Zeros(_) => 1,
            PackedDeltaRun::OneByte(vals) => vals.len() as u16 + 1,
            PackedDeltaRun::TwoBytes(vals) => vals.len() as u16 * 2 + 1,
        }
    }
}

impl FontWrite for PackedDeltaRun<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        self.compute_flag().write_into(writer);
        match self {
            PackedDeltaRun::Zeros(_) => (),
            PackedDeltaRun::OneByte(deltas) => {
                deltas.iter().for_each(|v| (*v as i8).write_into(writer))
            }
            PackedDeltaRun::TwoBytes(deltas) => deltas.iter().for_each(|v| v.write_into(writer)),
        }
    }
}

impl crate::validate::Validate for PackedPointNumbers {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        if let PackedPointNumbers::Some(pts) = self {
            if pts.len() > 0x7FFF {
                ctx.report("length cannot be stored in 15 bites");
            }
        }
    }
}

impl FontWrite for PackedPointNumbers {
    fn write_into(&self, writer: &mut TableWriter) {
        // compute the actual count:
        match self.as_slice().len() {
            len @ 0..=127 => (len as u8).write_into(writer),
            len => (len as u16).write_into(writer),
        }
        for run in self.iter_runs() {
            run.write_into(writer);
        }
    }
}

impl PackedPointNumbers {
    /// Compute the number of bytes required to encode these points
    pub(crate) fn compute_size(&self) -> u16 {
        let mut count = match self {
            PackedPointNumbers::All => return 1,
            PackedPointNumbers::Some(pts) if pts.len() < 128 => 1u16,
            PackedPointNumbers::Some(_) => 2,
        };
        for run in self.iter_runs() {
            count = count.checked_add(run.compute_size()).unwrap();
        }
        count
    }

    fn as_slice(&self) -> &[u16] {
        match self {
            PackedPointNumbers::All => &[],
            PackedPointNumbers::Some(pts) => pts.as_slice(),
        }
    }

    fn iter_runs(&self) -> impl Iterator<Item = PackedPointRun> {
        const U8_MAX: u16 = u8::MAX as u16;
        const MAX_POINTS_PER_RUN: usize = 128;

        let mut points = match self {
            PackedPointNumbers::Some(pts) => pts.as_slice(),
            PackedPointNumbers::All => &[],
        };

        let mut prev_point = 0u16;

        // split a run off the front of points:
        // - if point is more than 255 away from prev, we're using words
        std::iter::from_fn(move || {
            let next = points.first()?;
            let are_words = (next - prev_point) > U8_MAX;
            let run_len = points
                .iter()
                .take(MAX_POINTS_PER_RUN)
                .scan(prev_point, |prev, point| {
                    let take_this = if are_words {
                        (point - *prev) > U8_MAX
                    } else {
                        (point - *prev) <= U8_MAX
                    };
                    *prev = *point;
                    take_this.then_some(point)
                })
                .count();

            let (head, tail) = points.split_at(run_len);
            points = tail;
            let last_point = prev_point;
            prev_point = head.last().copied().unwrap();

            Some(PackedPointRun {
                last_point,
                are_words,
                points: head,
            })
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PackedPointRun<'a> {
    last_point: u16,
    are_words: bool,
    points: &'a [u16],
}

impl PackedPointRun<'_> {
    fn compute_size(&self) -> u16 {
        const LEN_BYTE: u16 = 1;
        let per_point_len = if self.are_words { 2 } else { 1 };
        self.points.len() as u16 * per_point_len + LEN_BYTE
    }
}

impl FontWrite for PackedPointRun<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        assert!(!self.points.is_empty() && self.points.len() <= 128);
        let mut len = self.points.len() as u8 - 1;
        if self.are_words {
            len |= 0x80;
        }
        len.write_into(writer);
        let mut last_point = self.last_point;
        for point in self.points {
            let delta = point - last_point;
            last_point = *point;
            if self.are_words {
                delta.write_into(writer);
            } else {
                debug_assert!(delta <= u8::MAX as u16);
                (delta as u8).write_into(writer);
            }
        }
    }
}

impl FontWrite for TupleIndex {
    fn write_into(&self, writer: &mut TableWriter) {
        self.bits().write_into(writer)
    }
}

//hack: unclear if we're even going to do any codegen for writing, but
//for the time being this lets us compile
impl<'a> FromObjRef<Option<read_fonts::tables::variations::Tuple<'a>>> for Vec<F2Dot14> {
    fn from_obj_ref(
        from: &Option<read_fonts::tables::variations::Tuple<'a>>,
        _data: FontData,
    ) -> Self {
        from.as_ref()
            .map(|tup| tup.values.iter().map(BigEndian::get).collect())
            .unwrap_or_default()
    }
}

impl Tuple {
    pub fn len(&self) -> u16 {
        self.values.len().try_into().unwrap()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Clone, Debug)]
pub enum IupError {
    DeltaCoordLengthMismatch {
        num_deltas: usize,
        num_coords: usize,
    },
    NotEnoughCoords(usize),
    CoordEndsMismatch {
        num_coords: usize,
        expected_num_coords: usize,
    },
    AchievedInvalidState(String),
}

/// Check if IUP _might_ be possible. If not then we *must* encode the value at this index.
///
/// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L238-L290>
fn must_encode_at(deltas: &[Vec2], coords: &[Point], tolerance: f64, at: usize) -> bool {
    let ld = *deltas.wrapping_prev(at);
    let d = deltas[at];
    let nd = *deltas.wrapping_next(at);
    let lc = *coords.wrapping_prev(at);
    let c = coords[at];
    let nc = *coords.wrapping_next(at);

    for axis in [Axis2D::X, Axis2D::Y] {
        let (ldj, lcj) = (ld.get(axis), lc.get(axis));
        let (dj, cj) = (d.get(axis), c.get(axis));
        let (ndj, ncj) = (nd.get(axis), nc.get(axis));
        let (c1, c2, d1, d2) = if lcj <= ncj {
            (lcj, ncj, ldj, ndj)
        } else {
            (ncj, lcj, ndj, ldj)
        };
        // If the two coordinates are the same, then the interpolation
        // algorithm produces the same delta if both deltas are equal,
        // and zero if they differ.
        match (c1, c2) {
            _ if c1 == c2 => {
                if (d1 - d2).abs() > tolerance && dj.abs() > tolerance {
                    return true;
                }
            }
            _ if c1 <= cj && cj <= c2 => {
                // and c1 != c2
                // If coordinate for current point is between coordinate of adjacent
                // points on the two sides, but the delta for current point is NOT
                // between delta for those adjacent points (considering tolerance
                // allowance), then there is no way that current point can be IUP-ed.
                if !(d1.min(d2) - tolerance <= dj && dj <= d1.max(d2) + tolerance) {
                    return true;
                }
            }
            _ => {
                // cj < c1 or c2 < cj
                // Otherwise, the delta should either match the closest, or have the
                // same sign as the interpolation of the two deltas.
                if d1 != d2 && dj.abs() > tolerance {
                    if cj < c1 {
                        if ((dj - d1).abs() > tolerance) && (dj - tolerance < d1) != (d1 < d2) {
                            return true;
                        }
                    } else if ((dj - d2).abs() > tolerance) && (d2 < dj + tolerance) != (d1 < d2) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Indices of deltas that must be encoded explicitly because they can't be interpolated.
///
/// These deltas must be encoded explicitly. That allows us the dynamic
/// programming solution clear stop points which speeds it up considerably.
///
/// Rust port of <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L218>
fn iup_must_encode(
    deltas: &[Vec2],
    coords: &[Point],
    tolerance: f64,
) -> Result<HashSet<usize>, IupError> {
    Ok((0..deltas.len())
        .rev()
        .filter(|i| must_encode_at(deltas, coords, tolerance, *i))
        .collect())
}

// TODO: use for might_iup? - take point and vec and coord
#[derive(Copy, Clone, Debug)]
enum Axis2D {
    X,
    Y,
}

/// Some of the fonttools code loops over 0,1 to access x/y.
///
/// Attempt to provide a nice way to do the same in Rust.
trait Coord {
    fn get(&self, axis: Axis2D) -> f64;
    fn set(&mut self, coord: Axis2D, value: f64);
}

impl Coord for Point {
    fn get(&self, axis: Axis2D) -> f64 {
        match axis {
            Axis2D::X => self.x,
            Axis2D::Y => self.y,
        }
    }

    fn set(&mut self, axis: Axis2D, value: f64) {
        match axis {
            Axis2D::X => self.x = value,
            Axis2D::Y => self.y = value,
        }
    }
}

impl Coord for Vec2 {
    fn get(&self, axis: Axis2D) -> f64 {
        match axis {
            Axis2D::X => self.x,
            Axis2D::Y => self.y,
        }
    }

    fn set(&mut self, axis: Axis2D, value: f64) {
        match axis {
            Axis2D::X => self.x = value,
            Axis2D::Y => self.y = value,
        }
    }
}

/// Given two reference coordinates `rc1` & `rc2` and their respective
/// delta vectors `rd1` & `rd2`, returns interpolated deltas for the set of
/// coordinates `coords`.
///
/// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L53>
fn iup_segment(coords: &[Point], rc1: Point, rd1: Vec2, rc2: Point, rd2: Vec2) -> Vec<Vec2> {
    // rc1 = reference coord 1
    // rd1 = reference delta 1

    let n = coords.len();
    let mut result = vec![Vec2::default(); n];
    for axis in [Axis2D::X, Axis2D::Y] {
        let c1 = rc1.get(axis);
        let c2 = rc2.get(axis);
        let d1 = rd1.get(axis);
        let d2 = rd2.get(axis);

        if c1 == c2 {
            let value = if d1 == d2 { d1 } else { 0.0 };
            for r in result.iter_mut() {
                r.set(axis, value);
            }
            continue;
        }

        let (c1, c2, d1, d2) = if c1 > c2 {
            (c2, c1, d2, d1) // flip
        } else {
            (c1, c2, d1, d2) // nop
        };

        // # c1 < c2
        let scale = (d2 - d1) / (c2 - c1);
        for (idx, point) in coords.iter().enumerate() {
            let c = point.get(axis);
            let d = if c <= c1 {
                d1
            } else if c >= c2 {
                d2
            } else {
                // Interpolate
                d1 + (c - c1) * scale
            };
            result[idx].set(axis, d);
        }
    }
    result
}

/// Checks if the deltas for points at `i` and `j` (`i < j`) be
/// used to interpolate deltas for points in between them within
/// provided error tolerance
///
/// See [iup_contour_optimize_dp] comments for context on from/to range restrictions.
///
/// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L187>
fn can_iup_in_between(
    deltas: &[Vec2],
    coords: &[Point],
    tolerance: f64,
    from: isize,
    to: isize,
) -> Result<bool, IupError> {
    if from < -1 || to <= from || to - from < 2 {
        return Err(IupError::AchievedInvalidState(format!(
            "bad from/to: {from}..{to}"
        )));
    }
    // from is >= -1 so from + 1 is a valid usize
    // to > from so to is a valid usize
    // from -1 is taken to mean the last entry
    let to = to as usize;
    let (rc1, rd1) = if from < 0 {
        (*coords.last().unwrap(), *deltas.last().unwrap())
    } else {
        (coords[from as usize], deltas[from as usize])
    };
    let iup_values = iup_segment(
        &coords[(from + 1) as usize..to],
        rc1,
        rd1,
        coords[to],
        deltas[to],
    );

    let real_values = &deltas[(from + 1) as usize..to];

    Ok(real_values
        .iter()
        .zip(iup_values)
        .all(|(d, i)| (d.x - i.x).abs() <= tolerance && (d.y - i.y).abs() <= tolerance))
}

/// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L327>
fn iup_initial_lookback(deltas: &[Vec2]) -> usize {
    std::cmp::min(deltas.len(), 8usize) // no more than 8!
}

#[derive(Debug, PartialEq)]
struct OptimizeDpResult {
    costs: Vec<i32>,
    chain: Vec<Option<usize>>,
}

/// Straightforward Dynamic-Programming.  For each index i, find least-costly encoding of
/// points 0 to i where i is explicitly encoded.  We find this by considering all previous
/// explicit points j and check whether interpolation can fill points between j and i.
///
/// Note that solution always encodes last point explicitly.  Higher-level is responsible
/// for removing that restriction.
///
/// As major speedup, we stop looking further whenever we see a point we are certain requires explicit encoding.
/// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L308>
fn iup_contour_optimize_dp(
    deltas: &[Vec2],
    coords: &[Point],
    tolerance: f64,
    must_encode: &HashSet<usize>,
    lookback: usize,
) -> Result<OptimizeDpResult, IupError> {
    let n = deltas.len();
    let lookback = lookback as isize;
    let mut costs = Vec::with_capacity(n);
    let mut chain: Vec<_> = (0..n)
        .map(|i| if i > 0 { Some(i - 1) } else { None })
        .collect();

    // n < 2 is degenerate
    if n < 2 {
        return Ok(OptimizeDpResult { costs, chain });
    }

    for i in 0..n {
        let mut best_cost = if i > 0 { costs[i - 1] } else { 0 } + 1;

        costs.push(best_cost);
        if i > 0 && must_encode.contains(&(i - 1)) {
            continue;
        }

        // python inner loop is j in range(i - 2, max(i - lookback, -2), -1)
        //      start at no less than -2, step -1 toward no less than -2
        //      lookback is either deltas.len() or deltas.len()/2 (because we tried repeating the contour)
        //          deltas must have more than 1 point to even try, so lookback at least 2
        // how does this play out? - slightly non-obviously one might argue
        // costs starts as {-1:0}
        // at i=0
        //      best_cost is set to costs[-1] + 1 which is 1
        //      costs[0] = best_cost, costs is {-1:0, 0:1}
        //      j in range(-2, max(0 - at least 2, -2), -1), so range(-2, -2, -1), which is empty
        // at i=1
        //      best_cost is set to costs[-1] + 1 which is 2
        //      costs[i] = best_cost, costs is {-1:0, 0:1, 1:2}
        //      j in range(-1, max(1 - at least 2, -2), -1), so it must be one of:
        //          range(-1, -2, -1), range(-1, -1, -1)
        //          only range(-1, -2, -1) has any values, -1
        //          when j = -1
        //              cost = costs[-1] + 1 will set cost to 1, which is < best_cost
        //              call can_iup_in_between for -1, 1
        // from i=2 onward we walk from >=0 to >=-2 non-inclusive, so can_iup_in_between
        // will only see indices >= -1. In Python -1 reads the last point, which must be encoded.

        // Python loops from high (inclusive) to low (exclusive) stepping -1
        // To match, we loop from low+1 to high+1 to exclude low and include high
        // and reverse to mimic the step
        let j_min = std::cmp::max(i as isize - lookback, -2);
        let j_max = i as isize - 2;
        for j in (j_min + 1..j_max + 1).rev() {
            let (cost, must_encode) = if j >= 0 {
                (costs[j as usize] + 1, must_encode.contains(&(j as usize)))
            } else {
                (1, false)
            };
            if cost < best_cost && can_iup_in_between(deltas, coords, tolerance, j, i as isize)? {
                best_cost = cost;
                costs[i] = best_cost;
                chain[i] = if j >= 0 { Some(j as usize) } else { None };
            }
            if must_encode {
                break;
            }
        }
    }

    Ok(OptimizeDpResult { costs, chain })
}

/// For contour with coordinates `coords`, optimize a set of delta values `deltas` within error `tolerance`.
///
/// Returns delta vector that has most number of None items instead of the input delta.
/// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L369>
fn iup_contour_optimize(
    deltas: &mut [Vec2],
    coords: &mut [Point],
    tolerance: f64,
) -> Result<Vec<Option<Vec2>>, IupError> {
    if deltas.len() != coords.len() {
        return Err(IupError::DeltaCoordLengthMismatch {
            num_deltas: deltas.len(),
            num_coords: coords.len(),
        });
    }

    let n = deltas.len();

    // Get the easy cases out of the way
    // Easy: all points are the same or there are no points
    // This covers the case when there is only one point
    let Some(first_delta) = deltas.get(0) else {
        return Ok(Vec::new());
    };
    if deltas.iter().all(|d| d == first_delta) {
        let mut result = vec![None; n];
        result[0] = Some(*first_delta);
        return Ok(result);
    }

    // Solve the general problem using Dynamic Programming
    let must_encode = iup_must_encode(deltas, coords, tolerance)?;

    // The iup_contour_optimize_dp() routine returns the optimal encoding
    // solution given the constraint that the last point is always encoded.
    // To remove this constraint, we use two different methods, depending on
    // whether forced set is non-empty or not:

    // Debugging: Make the next if always take the second branch and observe
    // if the font size changes (reduced); that would mean the forced-set
    // has members it should not have.
    let encode = if !must_encode.is_empty() {
        // Setup for iup_contour_optimize_dp
        // We know at least one point *must* be encoded so rotate such that last point is encoded
        // rot must be > 0 so this is rightwards
        let mid = n - 1 - must_encode.iter().max().unwrap();

        deltas.rotate_right(mid);
        coords.rotate_right(mid);
        let must_encode: HashSet<usize> = must_encode.iter().map(|idx| (idx + mid) % n).collect();
        let dp_result = iup_contour_optimize_dp(
            deltas,
            coords,
            tolerance,
            &must_encode,
            iup_initial_lookback(deltas),
        )?;

        // Assemble solution
        let mut encode = HashSet::new();

        let mut i = n - 1;
        while let Some(next) = dp_result.chain[i] {
            encode.insert(i);
            i = next;
        }

        if !encode.is_superset(&must_encode) {
            return Err(IupError::AchievedInvalidState(format!(
                "{encode:?} should contain {must_encode:?}"
            )));
        }

        encode
    } else {
        // Repeat the contour an extra time, solve the new case, then look for solutions of the
        // circular n-length problem in the solution for new linear case.  I cannot prove that
        // this always produces the optimal solution...
        let mut deltas_twice = Vec::with_capacity(2 * n);
        deltas_twice.extend_from_slice(deltas);
        deltas_twice.extend_from_slice(deltas);
        let mut coords_twice = Vec::with_capacity(2 * n);
        coords_twice.extend_from_slice(coords);
        coords_twice.extend_from_slice(coords);

        let dp_result = iup_contour_optimize_dp(
            &deltas_twice,
            &coords_twice,
            tolerance,
            &must_encode,
            iup_initial_lookback(deltas),
        )?;

        let mut best_sol = None;
        let mut best_cost = (n + 1) as i32;

        for start in n - 1..dp_result.costs.len() - 1 {
            // Assemble solution
            let mut solution = HashSet::new();
            let mut i = Some(start);
            while i > Some(start.saturating_sub(n)) {
                let idx = i.unwrap();
                solution.insert(idx % n);
                i = dp_result.chain[idx];
            }
            if i == Some(start.saturating_sub(n)) {
                // Python reads [-1] to get 0, usize doesn't like that
                let cost = dp_result.costs[start]
                    - if n < start {
                        dp_result.costs[start - n]
                    } else {
                        0
                    };
                if cost <= best_cost {
                    best_sol = Some(solution);
                    best_cost = cost;
                }
            }
        }

        let encode = best_sol.ok_or(IupError::AchievedInvalidState(
            "No best solution identified".to_string(),
        ))?;

        if !encode.is_superset(&must_encode) {
            return Err(IupError::AchievedInvalidState(format!(
                "{encode:?} should contain {must_encode:?}"
            )));
        }

        encode
    };

    Ok((0..n)
        .map(|i| {
            if encode.contains(&i) {
                Some(deltas[i])
            } else {
                None
            }
        })
        .collect())
}

const NUM_PHANTOM_POINTS: usize = 4;

/// For the outline given in `coords`, with contour endpoints given
/// `ends`, optimize a set of delta values `deltas` within error `tolerance`.
///
/// Returns delta vector that has most number of None items instead of
/// the input delta.
///
/// See:
/// * <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Lib/fontTools/varLib/iup.py#L470>
/// * <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#inferred-deltas-for-un-referenced-point-numbers>
pub fn iup_delta_optimize(
    deltas: &mut [Vec2],
    coords: &mut [Point],
    tolerance: f64,
    contour_ends: &[usize],
) -> Result<Vec<Option<Vec2>>, IupError> {
    let num_coords = coords.len();
    if num_coords < NUM_PHANTOM_POINTS {
        return Err(IupError::NotEnoughCoords(num_coords));
    }
    if deltas.len() != coords.len() {
        return Err(IupError::DeltaCoordLengthMismatch {
            num_deltas: deltas.len(),
            num_coords: coords.len(),
        });
    }

    let mut contour_ends = contour_ends.to_vec();
    contour_ends.sort();

    let expected_num_coords = contour_ends
        .last()
        .copied()
        //.map(|v| v + 1)
        .unwrap_or_default()
        + NUM_PHANTOM_POINTS;
    if num_coords != expected_num_coords {
        return Err(IupError::CoordEndsMismatch {
            num_coords,
            expected_num_coords,
        });
    }

    for offset in (1..=4).rev() {
        contour_ends.push(num_coords.saturating_sub(offset));
    }

    let mut result = Vec::with_capacity(num_coords);
    let mut start = 0;
    for end in contour_ends {
        let contour = iup_contour_optimize(
            &mut deltas[start..=end],
            &mut coords[start..=end],
            tolerance,
        )?;
        result.extend_from_slice(&contour);
        assert_eq!(contour.len() + start, end + 1);
        start = end + 1;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_pack_words() {
        let thing = PackedPointNumbers::Some(vec![1002, 2002, 8408, 12228]);

        let runs = thing.iter_runs().collect::<Vec<_>>();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].are_words);
        assert_eq!(runs[0].last_point, 0);
        assert_eq!(runs[0].points, &[1002, 2002, 8408, 12228]);
    }

    #[test]
    fn serialize_packed_points() {
        let thing = PackedPointNumbers::Some(vec![1002, 2002, 8408, 12228]);

        let bytes = crate::dump_table(&thing).unwrap();
        assert_eq!(thing.compute_size() as usize, bytes.len());
        let (read, _) = read_fonts::tables::variations::PackedPointNumbers::split_off_front(
            FontData::new(&bytes),
        );
        assert_eq!(thing.as_slice(), read.iter().collect::<Vec<_>>());
    }

    #[test]
    fn point_pack_runs() {
        let thing = PackedPointNumbers::Some(vec![5, 25, 225, 1002, 2002, 2008, 2228]);

        let runs = thing.iter_runs().collect::<Vec<_>>();
        assert!(!runs[0].are_words);
        assert_eq!(runs[0].last_point, 0);
        assert_eq!(runs[0].points, &[5, 25, 225]);

        assert!(runs[1].are_words);
        assert_eq!(runs[1].last_point, 225);
        assert_eq!(runs[1].points, &[1002, 2002]);

        assert!(!runs[2].are_words);
        assert_eq!(runs[2].last_point, 2002);
        assert_eq!(runs[2].points, &[2008, 2228]);

        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn point_pack_long_runs() {
        let mut numbers = vec![0u16; 130];
        numbers.extend(1u16..=130u16);
        let thing = PackedPointNumbers::Some(numbers);

        let runs = thing.iter_runs().collect::<Vec<_>>();
        assert!(!runs[0].are_words);
        assert_eq!(runs[0].points.len(), 128);
        assert_eq!(runs[1].last_point, 0);
        assert_eq!(runs[1].points.len(), 128);
        assert_eq!(runs[2].last_point, 126);
        assert_eq!(runs[2].points, &[127, 128, 129, 130]);
        assert!(runs.get(3).is_none());
    }

    #[test]
    fn point_pack_write() {
        let thing = PackedPointNumbers::Some(vec![5, 25, 225, 1002, 2002, 2008, 2228]);

        let bytes = crate::dump_table(&thing).unwrap();
        assert_eq!(thing.compute_size() as usize, bytes.len());
        let (read, _) = read_fonts::tables::variations::PackedPointNumbers::split_off_front(
            FontData::new(&bytes),
        );
        assert_eq!(thing.as_slice(), read.iter().collect::<Vec<_>>());
    }

    static PACKED_DELTA_BYTES: &[u8] = &[
        0x03, 0x0A, 0x97, 0x00, 0xC6, 0x87, 0x41, 0x10, 0x22, 0xFB, 0x34,
    ];

    // <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
    #[test]
    fn packed_deltas_spec_runs() {
        let deltas = PackedDeltas::new(vec![10, -105, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 4130, -1228]);
        let runs = deltas.iter_runs().collect::<Vec<_>>();
        assert_eq!(runs[0], PackedDeltaRun::OneByte(&[10, -105, 0, -58]));
        assert_eq!(runs[1], PackedDeltaRun::Zeros(8));
        assert_eq!(runs[2], PackedDeltaRun::TwoBytes(&[4130, -1228]));
        assert!(runs.get(3).is_none());
    }

    #[test]
    fn packed_deltas_spec_write() {
        let deltas = PackedDeltas::new(vec![10, -105, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 4130, -1228]);
        let bytes = crate::dump_table(&deltas).unwrap();
        assert_eq!(bytes, PACKED_DELTA_BYTES);
        let read = read_fonts::tables::variations::PackedDeltas::new(FontData::new(&bytes));
        let decoded = read.iter().collect::<Vec<_>>();
        assert_eq!(deltas.deltas.len(), decoded.len());
        assert_eq!(deltas.deltas, decoded);
        assert_eq!(bytes, PACKED_DELTA_BYTES);
    }

    #[test]
    fn empty_deltas() {
        let deltas = PackedDeltas::new(vec![]);
        let bytes = crate::dump_table(&deltas).unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn lots_of_zero() {
        let num_zeroes = 65;
        let deltas = PackedDeltas::new(vec![0; num_zeroes]);
        assert_eq!(
            vec![PackedDeltaRun::Zeros(64), PackedDeltaRun::Zeros(1)],
            deltas.iter_runs().collect::<Vec<_>>()
        );
    }

    #[test]
    fn respect_my_run_length_authority() {
        let values = (1..201).collect::<Vec<_>>();
        let deltas = PackedDeltas::new(values);
        assert_eq!(
            vec![
                // 64 entries per run please and thank you
                PackedDeltaRun::OneByte(&(1..65).collect::<Vec<i16>>()),
                // 63 entries this time because at 128 we switch to 2 bytes
                PackedDeltaRun::OneByte(&(65..128).collect::<Vec<i16>>()),
                // 64 per run again
                PackedDeltaRun::TwoBytes(&(128..192).collect::<Vec<i16>>()),
                // tail
                PackedDeltaRun::TwoBytes(&(192..=200).collect::<Vec<i16>>()),
            ],
            deltas.iter_runs().collect::<Vec<_>>()
        )
    }

    struct IupScenario {
        deltas: Vec<Vec2>,
        coords: Vec<Point>,
        expected_must_encode: HashSet<usize>,
    }

    impl IupScenario {
        /// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Tests/varLib/iup_test.py#L113>
        fn assert_must_encode(&self) {
            assert_eq!(
                self.expected_must_encode,
                iup_must_encode(&self.deltas, &self.coords, f64::EPSILON).unwrap()
            );
        }

        /// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Tests/varLib/iup_test.py#L116-L120>
        fn assert_optimize_dp(&self) {
            let must_encode = iup_must_encode(&self.deltas, &self.coords, f64::EPSILON).unwrap();
            let lookback = iup_initial_lookback(&self.deltas);
            let r1 = iup_contour_optimize_dp(
                &self.deltas,
                &self.coords,
                f64::EPSILON,
                &must_encode,
                lookback,
            )
            .unwrap();
            let must_encode = HashSet::new();
            let r2 = iup_contour_optimize_dp(
                &self.deltas,
                &self.coords,
                f64::EPSILON,
                &must_encode,
                lookback,
            )
            .unwrap();

            assert_eq!(r1, r2);
        }

        /// No Python equivalent
        fn assert_optimize_contour(&self) {
            let mut deltas = self.deltas.clone();
            let mut coords = self.coords.clone();
            iup_contour_optimize(&mut deltas, &mut coords, f64::EPSILON).unwrap();
        }
    }

    /// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Tests/varLib/iup_test.py#L15>
    fn iup_scenario1() -> IupScenario {
        IupScenario {
            deltas: vec![(0.0, 0.0).into()],
            coords: vec![(1.0, 2.0).into()],
            expected_must_encode: HashSet::new(),
        }
    }

    /// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Tests/varLib/iup_test.py#L16>
    fn iup_scenario2() -> IupScenario {
        IupScenario {
            deltas: vec![(0.0, 0.0).into(), (0.0, 0.0).into(), (0.0, 0.0).into()],
            coords: vec![(1.0, 2.0).into(), (3.0, 2.0).into(), (2.0, 3.0).into()],
            expected_must_encode: HashSet::new(),
        }
    }

    /// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Tests/varLib/iup_test.py#L17-L21>
    fn iup_scenario3() -> IupScenario {
        IupScenario {
            deltas: vec![
                (1.0, 1.0).into(),
                (-1.0, 1.0).into(),
                (-1.0, -1.0).into(),
                (1.0, -1.0).into(),
            ],
            coords: vec![
                (0.0, 0.0).into(),
                (2.0, 0.0).into(),
                (2.0, 2.0).into(),
                (0.0, 2.0).into(),
            ],
            expected_must_encode: HashSet::new(),
        }
    }

    /// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Tests/varLib/iup_test.py#L22-L52>
    fn iup_scenario4() -> IupScenario {
        IupScenario {
            deltas: vec![
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (0.0, 0.0).into(),
                (0.0, 0.0).into(),
                (0.0, 0.0).into(),
                (0.0, 0.0).into(),
                (0.0, 0.0).into(),
                (0.0, 0.0).into(),
                (-1.0, 0.0).into(),
            ],
            coords: vec![
                (-35.0, -152.0).into(),
                (-86.0, -101.0).into(),
                (-50.0, -65.0).into(),
                (0.0, -116.0).into(),
                (51.0, -65.0).into(),
                (86.0, -99.0).into(),
                (35.0, -151.0).into(),
                (87.0, -202.0).into(),
                (51.0, -238.0).into(),
                (-1.0, -187.0).into(),
                (-53.0, -239.0).into(),
                (-88.0, -205.0).into(),
            ],
            expected_must_encode: HashSet::from([11]),
        }
    }

    /// <https://github.com/fonttools/fonttools/blob/6a13bdc2e668334b04466b288d31179df1cff7be/Tests/varLib/iup_test.py#L53-L108>
    fn iup_scenario5() -> IupScenario {
        IupScenario {
            deltas: vec![
                (0.0, 0.0).into(),
                (1.0, 0.0).into(),
                (2.0, 0.0).into(),
                (2.0, 0.0).into(),
                (0.0, 0.0).into(),
                (1.0, 0.0).into(),
                (3.0, 0.0).into(),
                (3.0, 0.0).into(),
                (2.0, 0.0).into(),
                (2.0, 0.0).into(),
                (0.0, 0.0).into(),
                (0.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-3.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (0.0, 0.0).into(),
                (0.0, 0.0).into(),
                (-2.0, 0.0).into(),
                (-2.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-1.0, 0.0).into(),
                (-4.0, 0.0).into(),
            ],
            coords: vec![
                (330.0, 65.0).into(),
                (401.0, 65.0).into(),
                (499.0, 117.0).into(),
                (549.0, 225.0).into(),
                (549.0, 308.0).into(),
                (549.0, 422.0).into(),
                (549.0, 500.0).into(),
                (497.0, 600.0).into(),
                (397.0, 648.0).into(),
                (324.0, 648.0).into(),
                (271.0, 648.0).into(),
                (200.0, 620.0).into(),
                (165.0, 570.0).into(),
                (165.0, 536.0).into(),
                (165.0, 473.0).into(),
                (252.0, 407.0).into(),
                (355.0, 407.0).into(),
                (396.0, 407.0).into(),
                (396.0, 333.0).into(),
                (354.0, 333.0).into(),
                (249.0, 333.0).into(),
                (141.0, 268.0).into(),
                (141.0, 203.0).into(),
                (141.0, 131.0).into(),
                (247.0, 65.0).into(),
            ],
            expected_must_encode: HashSet::from([5, 15, 24]),
        }
    }

    /// The Python tests do not take the must encode empty branch of iup_contour_optimize,
    /// this test is meant to activate it by having enough points to be interesting and
    /// none of them must_encode.
    fn iup_scenario6() -> IupScenario {
        IupScenario {
            deltas: vec![
                (0.0, 0.0).into(),
                (1.0, 1.0).into(),
                (2.0, 2.0).into(),
                (3.0, 3.0).into(),
                (4.0, 4.0).into(),
                (5.0, 5.0).into(),
                (6.0, 6.0).into(),
                (7.0, 7.0).into(),
            ],
            coords: vec![
                (0.0, 0.0).into(),
                (10.0, 10.0).into(),
                (20.0, 20.0).into(),
                (30.0, 30.0).into(),
                (40.0, 40.0).into(),
                (50.0, 50.0).into(),
                (60.0, 60.0).into(),
                (70.0, 70.0).into(),
            ],
            expected_must_encode: HashSet::from([]),
        }
    }

    /// Another case with no must-encode items, this time a real one from a fontmake-rs test
    /// that was failing
    fn iup_scenario7() -> IupScenario {
        IupScenario {
            coords: vec![
                (242.0, 111.0),
                (314.0, 111.0),
                (314.0, 317.0),
                (513.0, 317.0),
                (513.0, 388.0),
                (314.0, 388.0),
                (314.0, 595.0),
                (242.0, 595.0),
                (242.0, 388.0),
                (43.0, 388.0),
                (43.0, 317.0),
                (242.0, 317.0),
                (0.0, 0.0),
                (557.0, 0.0),
                (0.0, 0.0),
                (0.0, 0.0),
            ]
            .into_iter()
            .map(|c| c.into())
            .collect(),
            deltas: vec![
                (-10.0, 0.0),
                (25.0, 0.0),
                (25.0, -18.0),
                (15.0, -18.0),
                (15.0, 18.0),
                (25.0, 18.0),
                (25.0, 1.0),
                (-10.0, 1.0),
                (-10.0, 18.0),
                (0.0, 18.0),
                (0.0, -18.0),
                (-10.0, -18.0),
                (0.0, 0.0),
                (15.0, 0.0),
                (0.0, 0.0),
                (0.0, 0.0),
            ]
            .into_iter()
            .map(|c| c.into())
            .collect(),
            expected_must_encode: HashSet::from([0]),
        }
    }

    #[test]
    fn iup_test_scenario01_must_encode() {
        iup_scenario1().assert_must_encode();
    }

    #[test]
    fn iup_test_scenario02_must_encode() {
        iup_scenario2().assert_must_encode();
    }

    #[test]
    fn iup_test_scenario03_must_encode() {
        iup_scenario3().assert_must_encode();
    }

    #[test]
    fn iup_test_scenario04_must_encode() {
        iup_scenario4().assert_must_encode();
    }

    #[test]
    fn iup_test_scenario05_must_encode() {
        iup_scenario5().assert_must_encode();
    }

    #[test]
    fn iup_test_scenario06_must_encode() {
        iup_scenario6().assert_must_encode();
    }

    #[test]
    fn iup_test_scenario07_must_encode() {
        iup_scenario7().assert_must_encode();
    }

    #[test]
    fn iup_test_scenario01_optimize() {
        iup_scenario1().assert_optimize_dp();
    }

    #[test]
    fn iup_test_scenario02_optimize() {
        iup_scenario2().assert_optimize_dp();
    }

    #[test]
    fn iup_test_scenario03_optimize() {
        iup_scenario3().assert_optimize_dp();
    }

    #[test]
    fn iup_test_scenario04_optimize() {
        iup_scenario4().assert_optimize_dp();
    }

    #[test]
    fn iup_test_scenario05_optimize() {
        iup_scenario5().assert_optimize_dp();
    }

    #[test]
    fn iup_test_scenario06_optimize() {
        iup_scenario6().assert_optimize_dp();
    }

    #[test]
    fn iup_test_scenario07_optimize() {
        iup_scenario7().assert_optimize_dp();
    }

    #[test]
    fn iup_test_scenario01_optimize_contour() {
        iup_scenario1().assert_optimize_contour();
    }

    #[test]
    fn iup_test_scenario02_optimize_contour() {
        iup_scenario2().assert_optimize_contour();
    }

    #[test]
    fn iup_test_scenario03_optimize_contour() {
        iup_scenario3().assert_optimize_contour();
    }

    #[test]
    fn iup_test_scenario04_optimize_contour() {
        iup_scenario4().assert_optimize_contour();
    }

    #[test]
    fn iup_test_scenario05_optimize_contour() {
        iup_scenario5().assert_optimize_contour();
    }

    #[test]
    fn iup_test_scenario06_optimize_contour() {
        iup_scenario6().assert_optimize_contour();
    }

    #[test]
    fn iup_test_scenario07_optimize_contour() {
        iup_scenario7().assert_optimize_contour();
    }
}
