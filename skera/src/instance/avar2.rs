// Copyright © 2017, 2023 Google, Inc.
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
//! Rebase avar2 inputs while preserving the original final-coordinate space.
//!
//! Follows HarfBuzz's avar2 offset compensation: delta rows are rebased in
//! intermediate space, then inverse-normalization offsets restore old outputs.
use super::{
    axes::map_float,
    rebase::{scalar, Triple},
    scalars::VariationScalars,
    AxisPlan, StorePlan,
};
use crate::SubsetError;
use read_fonts::{
    tables::{avar::SegmentMaps as ReadSegmentMaps, variations::DeltaSetIndex},
    types::{F2Dot14, Tag},
    FontRead, FontRef, TableProvider,
};
use std::collections::BTreeMap;
use write_fonts::tables::{
    avar::{Avar, SegmentMaps},
    variations::{
        DeltaSetIndexMap, ItemVariationData, ItemVariationStore, RegionAxisCoordinates,
        VariationRegion, VariationRegionList,
    },
};

type Region = Vec<(i16, i16, i16)>;
fn error() -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(b"avar"))
}

// A null avar2 store contributes zero deltas, but range compensation still
// needs to preserve the old final-coordinate space, as in HarfBuzz.
fn empty_store(axes: &AxisPlan) -> Result<Vec<u8>, SubsetError> {
    super::serialize::item_store(&ItemVariationStore {
        variation_region_list: VariationRegionList {
            axis_count: axes.pinned.len() as u16,
            variation_regions: Vec::new(),
        }
        .into(),
        item_variation_data: Vec::new(),
    })
    .map_err(|_| error())
}
fn add(row: &mut BTreeMap<Region, i64>, region: Region, delta: f64) {
    let delta = (delta + 0.5)
        .floor()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i64;
    if delta != 0 {
        *row.entry(region).or_default() += delta;
    }
}

type OffsetKnots = BTreeMap<F2Dot14, f64>;

fn normalize_value(value: f64, user: Triple) -> f64 {
    let value = value.clamp(user.0, user.2);
    if value == user.1 {
        0.
    } else if value < user.1 {
        if user.0 == user.1 {
            0.
        } else {
            (value - user.1) / (user.1 - user.0)
        }
    } else if user.1 == user.2 {
        0.
    } else {
        (value - user.1) / (user.2 - user.1)
    }
}

fn denormalize_value(value: f64, user: Triple) -> f64 {
    user.1
        + value
            * if value < 0. {
                user.1 - user.0
            } else {
                user.2 - user.1
            }
}

fn round_units(value: f64) -> i32 {
    let value = (value * 16384.) as f32 as f64;
    (value + 0.5).floor() as i32
}

// HarfBuzz's _avar2_map_new_mapping evaluates the instantiated map in double.
// Its inverse only supplies extra samples to the error estimator.
fn map_new_mapping(map: &ReadSegmentMaps, value: f64, inverse: bool) -> f64 {
    let mut before = None;
    for mapping in map.axis_value_maps() {
        let from = mapping.from_coordinate().to_f64();
        let to = mapping.to_coordinate().to_f64();
        let (from, to) = if inverse { (to, from) } else { (from, to) };
        if value == from {
            return to;
        }
        if value < from {
            return before.map_or(value - from + to, |(prev_from, prev_to)| {
                if from == prev_from {
                    prev_to
                } else {
                    prev_to + (to - prev_to) * (value - prev_from) / (from - prev_from)
                }
            });
        }
        before = Some((from, to));
    }
    before.map_or(value, |(from, to)| value - from + to)
}

fn eval_offset(knots: &OffsetKnots, value: f64) -> f64 {
    let mut before = None;
    for (&z, &offset) in knots {
        let z = z.to_f64();
        if value == z {
            return offset;
        }
        if value < z {
            return before.map_or(offset, |(prev_z, prev_offset)| {
                prev_offset + (offset - prev_offset) * (value - prev_z) / (z - prev_z)
            });
        }
        before = Some((z, offset));
    }
    before.map_or(0., |(_, offset)| offset)
}

// Sample the same uniform grid and kink preimages as HarfBuzz's
// _avar2_estimate_offset_error. This estimates requantization error; it is
// not an exact bound on the final avar2 variation-store evaluation.
fn estimate_offset_error(
    old_map: &ReadSegmentMaps,
    new_map: &ReadSegmentMaps,
    original: Triple,
    user: Triple,
    knots: &OffsetKnots,
) -> u32 {
    let samples = (0..257).map(|i| user.0 + (user.2 - user.0) * i as f64 / 256.);
    let old_kinks = old_map
        .axis_value_maps()
        .iter()
        .map(|m| denormalize_value(m.from_coordinate().to_f64(), original));
    let new_kinks = new_map
        .axis_value_maps()
        .iter()
        .map(|m| denormalize_value(m.from_coordinate().to_f64(), user));
    let offset_kinks = knots
        .keys()
        .map(|z| denormalize_value(map_new_mapping(new_map, z.to_f64(), true), user));
    samples
        .chain(old_kinks)
        .chain(new_kinks)
        .chain(offset_kinks)
        .filter(|u| user.0 <= *u && *u <= user.2)
        .map(|u| {
            let old_final = map_float(old_map, normalize_value(u, original), false);
            let z = map_new_mapping(new_map, normalize_value(u, user), false);
            round_units(old_final).abs_diff(round_units(z + eval_offset(knots, z)))
        })
        .max()
        .unwrap_or(0)
}

fn offset_knots(
    old_map: &ReadSegmentMaps,
    new_map: &ReadSegmentMaps,
    original: Triple,
    user: Triple,
    intermediate: Triple,
) -> OffsetKnots {
    let mut knots = BTreeMap::from([
        (F2Dot14::NEG_ONE, intermediate.0 + 1.),
        (F2Dot14::ZERO, intermediate.1),
        (F2Dot14::ONE, intermediate.2 - 1.),
    ]);
    let z = map_new_mapping(new_map, normalize_value(original.1, user), false);
    let z = F2Dot14::from_bits(round_units(z) as i16);
    if F2Dot14::NEG_ONE < z && z < F2Dot14::ONE {
        // Like _avar2_add_knot, first insertion wins, preserving anchors.
        knots.entry(z).or_insert(-z.to_f64());
    }
    let mut with_breakpoints = knots.clone();
    for mapping in new_map.axis_value_maps() {
        let from = mapping.from_coordinate().to_f64();
        let z = mapping.to_coordinate();
        if [-1., 0., 1.].contains(&from) || !(F2Dot14::NEG_ONE < z && z < F2Dot14::ONE) {
            continue;
        }
        let u = denormalize_value(from, user);
        let old = map_float(old_map, normalize_value(u, original), false);
        let old = round_units(old) as f64 / 16384.;
        with_breakpoints.entry(z).or_insert(old - z.to_f64());
    }
    // Extra tents can increase rounding noise on steep segments. Keep the
    // breakpoint set only if it does not worsen the sampled residual.
    if with_breakpoints.len() != knots.len()
        && estimate_offset_error(old_map, new_map, original, user, &with_breakpoints)
            <= estimate_offset_error(old_map, new_map, original, user, &knots)
    {
        with_breakpoints
    } else {
        knots
    }
}

pub(super) struct Reachability {
    pub pins: Vec<Option<F2Dot14>>,
    pub ranges: Vec<Option<(i16, i16)>>,
}

// HarfBuzz's _compute_avar2_reachable_ranges: bound final coordinates over
// the retained intermediate-coordinate box and detect constant pins. Use
// the grid of tent breakpoints to preserve correlations between regions;
// fall back to conservative per-region intervals when the grid is too large.
pub(super) fn reachable_ranges(
    font: &FontRef,
    axes: &AxisPlan,
) -> Result<Reachability, SubsetError> {
    let mut pins = vec![None; axes.pinned.len()];
    let mut reachable = vec![None; axes.pinned.len()];
    // HarfBuzz cannot currently partially pin CFF2 blends. VARC remaps its
    // retained axis references and rejects changes to referenced axes.
    let detect_pins = font.cff2().is_err();
    let avar = font.avar().map_err(|_| error())?;
    let empty = empty_store(axes)?;
    let store = avar
        .var_store()
        .unwrap_or_else(|| {
            read_fonts::tables::variations::ItemVariationStore::read(empty.as_slice().into())
        })
        .map_err(|_| error())?;
    let map = avar.axis_index_map().transpose().map_err(|_| error())?;
    let fvar = font.fvar().map_err(|_| error())?;
    let old_axes = fvar.axes().map_err(|_| error())?;
    let ranges: Vec<_> = old_axes
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if axes.values.iter().any(|(tag, _)| *tag == a.axis_tag()) {
                (axes.normalized[i].0, axes.normalized[i].2)
            } else if a.flags() & 1 != 0 {
                (0., 0.)
            } else {
                (-1., 1.)
            }
        })
        .collect();
    let regions = store.variation_region_list().map_err(|_| error())?;
    if regions.axis_count() as usize != ranges.len() {
        return Err(error());
    }
    let regions: Vec<Vec<Triple>> = regions
        .variation_regions()
        .iter()
        .map(|r| {
            let r = r.map_err(|_| error())?;
            Ok(r.region_axes()
                .iter()
                .map(|a| {
                    Triple(
                        a.start_coord().to_f64(),
                        a.peak_coord().to_f64(),
                        a.end_coord().to_f64(),
                    )
                })
                .collect())
        })
        .collect::<Result<_, SubsetError>>()?;
    for (i, pin) in pins.iter_mut().enumerate() {
        let index = map
            .as_ref()
            .map(|m| m.get(i as u32))
            .transpose()
            .map_err(|_| error())?
            .unwrap_or(DeltaSetIndex {
                outer: 0,
                inner: i as u16,
            });
        // Missing rows contribute zero at runtime.
        let mut active = Vec::new();
        if let Some(Ok(data)) = store.item_variation_data().get(index.outer as usize) {
            if index.inner < data.item_count() {
                for (idx, delta) in data
                    .region_indexes()
                    .iter()
                    .zip(data.delta_set(index.inner))
                {
                    if delta != 0 {
                        let tents = regions.get(idx.get() as usize).ok_or_else(error)?;
                        active.push((delta as f64, tents));
                    }
                }
            }
        }
        // Add the target axis first, followed by the axes of valid active tents.
        let mut positions = vec![None; ranges.len()];
        let mut grid = vec![vec![ranges[i].0, ranges[i].1]];
        positions[i] = Some(0);
        let mut region_tents = Vec::new();
        for (_, region) in &active {
            let mut tents = Vec::new();
            for (axis, &tent) in region.iter().enumerate() {
                if tent.1 == 0. || tent.0 > tent.1 || tent.1 > tent.2 || tent.0 < 0. && tent.2 > 0.
                {
                    continue;
                }
                let pos = *positions[axis].get_or_insert_with(|| {
                    let pos = grid.len();
                    grid.push(vec![ranges[axis].0, ranges[axis].1]);
                    pos
                });
                for v in [tent.0, tent.1, tent.2] {
                    grid[pos].push(v.clamp(ranges[axis].0, ranges[axis].1));
                }
                tents.push((pos, tent));
            }
            region_tents.push(tents);
        }
        const MAX_GRID: usize = 1 << 14;
        let mut size = 1usize;
        let mut exact = true;
        for points in &mut grid {
            points.sort_by(f64::total_cmp);
            points.dedup();
            if size > MAX_GRID / points.len() {
                exact = false;
                break;
            }
            size *= points.len();
        }
        let (mut min, mut max) = (0., 0.);
        let (mut vmin, mut vmax) = (0., 0.);
        if exact {
            let mut odometer = vec![0; grid.len()];
            let mut first = true;
            loop {
                let delta: f64 = active
                    .iter()
                    .zip(&region_tents)
                    .map(|((delta, _), tents)| {
                        delta
                            * tents
                                .iter()
                                .map(|&(pos, tent)| scalar(grid[pos][odometer[pos]], tent))
                                .product::<f64>()
                    })
                    .sum();
                let value = grid[0][odometer[0]] + delta / 16384.;
                if first {
                    min = delta;
                    max = delta;
                    vmin = value;
                    vmax = value;
                    first = false;
                } else {
                    min = min.min(delta);
                    max = max.max(delta);
                    vmin = vmin.min(value);
                    vmax = vmax.max(value);
                }
                let mut k = 0;
                while k < odometer.len() {
                    odometer[k] += 1;
                    if odometer[k] < grid[k].len() {
                        break;
                    }
                    odometer[k] = 0;
                    k += 1;
                }
                if k == odometer.len() {
                    break;
                }
            }
        } else {
            for (delta, region) in &active {
                let (mut smin, mut smax) = (1., 1.);
                for (&tent, &(lo, hi)) in region.iter().zip(&ranges) {
                    let a = scalar(lo, tent);
                    let b = scalar(hi, tent);
                    smin *= a.min(b);
                    smax *= if lo <= tent.1 && tent.1 <= hi {
                        a.max(b).max(1.)
                    } else {
                        a.max(b)
                    };
                }
                if *delta > 0. {
                    min += smin * delta;
                    max += smax * delta;
                } else {
                    min += smax * delta;
                    max += smin * delta;
                }
            }
            vmin = ranges[i].0 + min / 16384.;
            vmax = ranges[i].1 + max / 16384.;
        }
        if detect_pins && axes.user_pinned[i] && min == max {
            // Round the delta alone, then add it to the quantized intermediate.
            // HarfBuzz uses roundf here, including its negative half ties.
            let intermediate = axes.normalized[i].1;
            let value = round_units(intermediate)
                + (min as f32 as f64 + 0.5).floor().clamp(-32768., 32768.) as i32;
            *pin = Some(F2Dot14::from_bits(value.clamp(-16384, 16384) as i16));
            continue;
        }
        // Pad outwards by one 2.14 unit against runtime rounding, then
        // quantize outwards. Only ranges narrower than [-1, 1] constrain.
        let lo = ((vmin * 16384. - 1.).clamp(-16384., 16384.)).floor() as i16;
        let hi = ((vmax * 16384. + 1.).clamp(-16384., 16384.)).ceil() as i16;
        if lo > -16384 || hi < 16384 {
            reachable[i] = Some((lo, hi));
        }
    }
    Ok(Reachability {
        pins,
        ranges: reachable,
    })
}

pub(super) fn instance(
    font: &FontRef,
    axes: &AxisPlan,
    segment_maps: Vec<SegmentMaps>,
    old: &read_fonts::tables::avar::Avar,
) -> Result<Avar, SubsetError> {
    let empty = empty_store(axes)?;
    let store = old
        .var_store()
        .unwrap_or_else(|| {
            read_fonts::tables::variations::ItemVariationStore::read(empty.as_slice().into())
        })
        .map_err(|_| error())?;
    let map = old.axis_index_map().transpose().map_err(|_| error())?;
    let old_maps = old
        .axis_segment_maps()
        .iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error())?;
    let fvar = font.fvar().map_err(|_| error())?;
    let old_axes = fvar.axis_instance_arrays().map_err(|_| error())?.axes();
    let count = axes.pinned.iter().filter(|&&p| !p).count();
    let neutral = vec![(0, 0, 0); count];

    // Pinned inputs are removed from this transform. Their output axes stay
    // present, with neutral supports inserted at those positions below.
    let mut inputs = axes.clone();
    inputs.pinned.clone_from(&axes.user_pinned);
    inputs.coords = axes
        .normalized
        .iter()
        .map(|t| F2Dot14::from_f64(t.1))
        .collect();
    let plan = StorePlan::new(&store, &inputs)?;
    let rebuilt = plan.rebuild(&store)?;
    let residual_regions: Vec<Region> = rebuilt
        .variation_region_list
        .as_ref()
        .variation_regions
        .iter()
        .map(|r| {
            let mut retained = r.region_axes.iter();
            axes.user_pinned
                .iter()
                .enumerate()
                .filter(|(i, _)| !axes.pinned[*i])
                .map(|(_, pinned)| {
                    if *pinned {
                        (0, 0, 0)
                    } else {
                        let r = retained.next().unwrap();
                        (
                            r.start_coord.to_bits(),
                            r.peak_coord.to_bits(),
                            r.end_coord.to_bits(),
                        )
                    }
                })
                .collect()
        })
        .collect();

    let scalars = VariationScalars::new(&store, &inputs.coords);
    let mut rows = Vec::new();
    for (i, axis) in old_axes.iter().enumerate() {
        let Some(new_index) = axes.new_index(i) else {
            continue;
        };
        let mut row = BTreeMap::new();
        let index = map
            .as_ref()
            .map(|m| m.get(i as u32))
            .transpose()
            .map_err(|_| error())?
            .unwrap_or(DeltaSetIndex {
                outer: 0,
                inner: i as u16,
            });
        // Invalid or missing rows have zero deltas, as at runtime.
        if let Some(Ok(data)) = store.item_variation_data().get(index.outer as usize) {
            if index.inner < data.item_count() {
                let deltas: Vec<_> = data.delta_set(index.inner).map(|v| v as f64).collect();
                let transform = plan
                    .transforms
                    .get(index.outer as usize)
                    .ok_or_else(error)?;
                for (region, delta) in transform.indices.iter().zip(transform.residual(&deltas)?) {
                    add(&mut row, residual_regions[*region as usize].clone(), delta);
                }
            }
        }
        let default_delta = scalars
            .as_ref()
            .and_then(|scalars| scalars.delta(&store, index))
            .map_or(0., super::round_f32);
        let u = axes.user[i];
        let original = (
            axis.min_value().to_f32() as f64,
            axis.default_value().to_f32() as f64,
            axis.max_value().to_f32() as f64,
        );
        let restricted = (u.0, u.1, u.2) != original || axes.user_pinned[i];
        let middle = if restricted { axes.normalized[i].1 } else { 0. };
        add(&mut row, neutral.clone(), middle * 16384. + default_delta);

        if restricted && !axes.user_pinned[i] {
            let t = axes.normalized[i];
            let bytes = super::serialize::segment_map(&segment_maps[new_index])?;
            let new_map = read_fonts::tables::avar::SegmentMaps::read(bytes.as_slice().into())
                .map_err(|_| error())?;
            let knots: Vec<_> = offset_knots(
                old_maps.get(i).ok_or_else(error)?,
                &new_map,
                Triple(original.0, original.1, original.2),
                u,
                t,
            )
            .into_iter()
            .collect();
            for (j, &(z, offset)) in knots.iter().enumerate() {
                if z == F2Dot14::ZERO {
                    continue;
                }
                let mut region = neutral.clone();
                region[new_index] = (
                    knots[j.saturating_sub(1)].0.to_bits(),
                    z.to_bits(),
                    knots[(j + 1).min(knots.len() - 1)].0.to_bits(),
                );
                add(&mut row, region, (offset - middle) * 16384.);
            }
        }
        row.retain(|_, d| *d != 0);
        rows.push(row);
    }

    let mut regions = BTreeMap::new();
    for row in &rows {
        for region in row.keys() {
            let index = u16::try_from(regions.len()).map_err(|_| error())?;
            regions.entry(region.clone()).or_insert(index);
        }
    }
    let mut by_index = vec![neutral; regions.len()];
    for (region, &index) in &regions {
        by_index[index as usize] = region.clone();
    }
    let mut data = Vec::new();
    for row in rows {
        if row.len() > 0x7fff {
            return Err(error());
        }
        let (region_indexes, delta_sets) = row
            .into_iter()
            .map(|(r, d)| {
                (
                    regions[&r],
                    (d.clamp(i32::MIN as i64, i32::MAX as i64) as i32).to_be_bytes(),
                )
            })
            .fold(
                (Vec::new(), Vec::new()),
                |(mut indices, mut bytes), (i, d)| {
                    indices.push(i);
                    bytes.extend(d);
                    (indices, bytes)
                },
            );
        data.push(
            ItemVariationData {
                item_count: 1,
                word_delta_count: 0x8000 | region_indexes.len() as u16,
                region_indexes,
                delta_sets,
            }
            .into(),
        );
    }
    let mut out = Avar::new(segment_maps);
    out.axis_index_map = Some(
        (0..count as u32)
            .map(|i| i << 16)
            .collect::<DeltaSetIndexMap>(),
    )
    .into();
    out.var_store = Some(ItemVariationStore {
        variation_region_list: VariationRegionList {
            axis_count: count as u16,
            variation_regions: by_index
                .into_iter()
                .map(|r| VariationRegion {
                    region_axes: r
                        .into_iter()
                        .map(|(a, b, c)| {
                            RegionAxisCoordinates::new(
                                F2Dot14::from_bits(a),
                                F2Dot14::from_bits(b),
                                F2Dot14::from_bits(c),
                            )
                        })
                        .collect(),
                })
                .collect(),
        }
        .into(),
        item_variation_data: data,
    })
    .into();
    Ok(out)
}
