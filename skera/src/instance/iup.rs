// Copyright © 2024 Google, Inc.
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
// Ported from HarfBuzz's hb-subset-instancer-iup.cc, itself a port of
// fontTools.varLib.iup. Keep the forced points, bounded lookback, and
// cyclic dynamic programming passes in the same order.

const MAX_LOOKBACK: usize = 8;
const MAX_CONTOUR_POINTS: usize = 512;
const TOLERANCE: f64 = 0.5 + 1e-10;
type Point = [f32; 2];
type Delta = [i32; 2];

fn forced_set(points: &[Point], deltas: &[Delta]) -> Vec<bool> {
    let n = points.len();
    let mut forced = vec![false; n];
    for i in (0..n).rev() {
        let prev = (i + n - 1) % n;
        let next = (i + 1) % n;
        for axis in 0..2 {
            let c = points[i][axis] as f64;
            let d = deltas[i][axis] as f64;
            let (a, b) = if points[prev][axis] <= points[next][axis] {
                (prev, next)
            } else {
                (next, prev)
            };
            let (c1, c2) = (points[a][axis] as f64, points[b][axis] as f64);
            let (d1, d2) = (deltas[a][axis] as f64, deltas[b][axis] as f64);
            let force = if c1 == c2 {
                (d1 - d2).abs() > TOLERANCE && d.abs() > TOLERANCE
            } else if c1 <= c && c <= c2 {
                !(d1.min(d2) - TOLERANCE <= d && d <= d1.max(d2) + TOLERANCE)
            } else if d1 != d2 && c < c1 {
                d.abs() > TOLERANCE
                    && (d - d1).abs() > TOLERANCE
                    && ((d - TOLERANCE < d1) != (d1 < d2))
            } else if d1 != d2 {
                d.abs() > TOLERANCE
                    && (d - d2).abs() > TOLERANCE
                    && ((d2 < d + TOLERANCE) != (d1 < d2))
            } else {
                false
            };
            if force {
                forced[i] = true;
                break;
            }
        }
    }
    forced
}

fn interpolate(value: f64, mut a: f64, mut b: f64, mut da: f64, mut db: f64) -> f64 {
    if a == b {
        return if da == db { da } else { 0. };
    }
    if a > b {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut da, &mut db);
    }
    if value <= a {
        da
    } else if value >= b {
        db
    } else {
        da + (value - a) * ((db - da) / (b - a))
    }
}

fn can_iup(points: &[Point], deltas: &[Delta], from: usize, to: usize, start: usize) -> bool {
    (start..to).all(|i| {
        let mut error_sq = 0.;
        for axis in 0..2 {
            let delta = interpolate(
                points[i][axis] as f64,
                points[from][axis] as f64,
                points[to][axis] as f64,
                deltas[from][axis] as f64,
                deltas[to][axis] as f64,
            );
            let error = delta - deltas[i][axis] as f64;
            error_sq += error * error;
        }
        error_sq <= TOLERANCE * TOLERANCE
    })
}

fn optimize_dp(
    points: &[Point],
    deltas: &[Delta],
    forced: &[bool],
    lookback: usize,
) -> (Vec<usize>, Vec<isize>) {
    let n = points.len();
    let mut costs = vec![0; n];
    let mut chain = vec![-1; n];
    let lookback = lookback.min(MAX_LOOKBACK);
    for i in 0..n {
        costs[i] = if i == 0 { 1 } else { costs[i - 1] + 1 };
        chain[i] = i as isize - 1;
        if i > 0 && forced[i - 1] {
            continue;
        }
        let last = (i as isize - lookback as isize + 1).max(-1);
        for j in (last..=i as isize - 2).rev() {
            let cost = if j < 0 { 1 } else { costs[j as usize] + 1 };
            let from = if j < 0 { n - 1 } else { j as usize };
            if cost < costs[i] && can_iup(points, deltas, from, i, (j + 1) as usize) {
                costs[i] = cost;
                chain[i] = j;
            }
            if j > 0 && forced[j as usize] {
                break;
            }
        }
    }
    (costs, chain)
}

fn optimize_contour(points: &[Point], deltas: &[Delta]) -> Vec<bool> {
    let n = points.len();
    // Bound the cyclic search just as HB does. A contour that exceeds the
    // bound must keep its references rather than lose its variation data.
    if n > MAX_CONTOUR_POINTS {
        return vec![true; n];
    }
    let mut selected = vec![false; n];
    if deltas.iter().all(|d| *d == [0, 0]) {
        return selected;
    }
    if n == 1 || deltas.iter().all(|d| *d == deltas[0]) {
        selected[0] = true;
        return selected;
    }
    let mut forced = forced_set(points, deltas);
    if let Some(last) = forced.iter().rposition(|&v| v) {
        let k = n - 1 - last;
        let mut points = points.to_vec();
        let mut deltas = deltas.to_vec();
        points.rotate_right(k);
        deltas.rotate_right(k);
        forced.rotate_right(k);
        let (_, chain) = optimize_dp(&points, &deltas, &forced, n);
        let mut i = n as isize - 1;
        while i >= 0 {
            selected[i as usize] = true;
            i = chain[i as usize];
        }
        selected.rotate_left(k);
    } else {
        let points = [points, points].concat();
        let deltas = [deltas, deltas].concat();
        let (costs, chain) = optimize_dp(&points, &deltas, &vec![false; n * 2], n);
        let mut best_cost = n + 1;
        for start in n - 1..n * 2 {
            let mut solution = vec![false; n];
            let mut i = start as isize;
            let stop = i - n as isize;
            while i > stop {
                solution[i as usize % n] = true;
                i = chain[i as usize];
            }
            if i == stop {
                let cost = costs[start] - if i < 0 { 0 } else { costs[i as usize] };
                if cost <= best_cost {
                    best_cost = cost;
                    selected = solution;
                }
            }
        }
        // If no cyclic solution is found, retain the dense deltas.
        if best_cost == n + 1 {
            selected.fill(true);
        }
    }
    selected
}

/// Contours contain inclusive endpoints; remaining points are independent
/// components or phantoms and cannot infer deltas from their neighbours.
pub(super) fn optimize(
    points: &[Point],
    deltas: &[Delta],
    contours: &[usize],
) -> Option<Vec<bool>> {
    if points.len() != deltas.len() {
        return None;
    }
    let mut selected = Vec::with_capacity(points.len());
    let mut start = 0;
    for &end in contours {
        if end < start || end >= points.len().saturating_sub(4) {
            return None;
        }
        selected.extend(optimize_contour(&points[start..=end], &deltas[start..=end]));
        start = end + 1;
    }
    selected.extend(deltas[start..].iter().map(|d| *d != [0, 0]));
    Some(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_reconstruction(points: &[Point], deltas: &[Delta], selected: &[bool]) {
        let refs: Vec<_> = (0..points.len()).filter(|&i| selected[i]).collect();
        if refs.is_empty() {
            assert!(deltas.iter().all(|d| *d == [0, 0]));
            return;
        }
        for (j, &a) in refs.iter().enumerate() {
            let b = refs[(j + 1) % refs.len()];
            let mut i = (a + 1) % points.len();
            while i != b {
                let mut error_sq = 0.;
                for axis in 0..2 {
                    let inferred = interpolate(
                        points[i][axis] as f64,
                        points[a][axis] as f64,
                        points[b][axis] as f64,
                        deltas[a][axis] as f64,
                        deltas[b][axis] as f64,
                    );
                    error_sq += (inferred - deltas[i][axis] as f64).powi(2);
                }
                assert!(error_sq <= TOLERANCE * TOLERANCE, "point {i}: {error_sq}");
                i = (i + 1) % points.len();
            }
        }
    }

    #[test]
    fn sparse_deltas_reconstruct_contours_within_tolerance() {
        let mut seed = 2266u32;
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed >> 16
        };
        for n in 2..40 {
            for kind in 0..32 {
                let points: Vec<_> = (0..n)
                    .map(|_| [(random() % 100) as f32, (random() % 100) as f32])
                    .collect();
                let deltas: Vec<_> = points
                    .iter()
                    .map(|p| match kind {
                        0 => [0, 0],
                        1 => [19, -19],
                        2 => [p[0] as i32, p[1] as i32],
                        _ => [(random() % 30) as i32 - 15, (random() % 30) as i32 - 15],
                    })
                    .collect();
                verify_reconstruction(&points, &deltas, &optimize_contour(&points, &deltas));
            }
        }
        let points = vec![[0., 0.]; MAX_CONTOUR_POINTS + 1];
        let deltas = vec![[19, -19]; points.len()];
        assert!(optimize_contour(&points, &deltas).iter().all(|&v| v));
    }

    #[test]
    fn components_and_phantoms_are_independent() {
        let points = vec![[0.; 2]; 8];
        let deltas = vec![
            [5, 0],
            [5, 0],
            [5, 0],
            [5, 0],
            [0, 0],
            [5, 0],
            [0, 0],
            [0, 0],
        ];
        assert_eq!(
            optimize(&points, &deltas, &[3]).unwrap(),
            [true, false, false, false, false, true, false, false]
        );
        assert_eq!(
            optimize(&points, &deltas, &[]).unwrap(),
            [true, true, true, true, false, true, false, false]
        );
        assert!(optimize(&points, &deltas, &[8]).is_none());
    }
}
