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
    axes::{axis_to_normalized, map_float},
    rebase::{scalar, Triple},
    AxisPlan, StorePlan,
};
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    read::{tables::variations::DeltaSetIndex, FontRead, FontRef, TableProvider},
    tables::{
        avar::{Avar, SegmentMaps},
        variations::{
            DeltaSetIndexMap, ItemVariationData, ItemVariationStore, RegionAxisCoordinates,
            VariationRegion, VariationRegionList,
        },
    },
    types::{F2Dot14, Tag},
};

type Region = Vec<(i16, i16, i16)>;
fn error() -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(b"avar"))
}
fn add(row: &mut BTreeMap<Region, i64>, region: Region, delta: f64) {
    let delta = delta.round().clamp(i32::MIN as f64, i32::MAX as f64) as i64;
    if delta != 0 {
        *row.entry(region).or_default() += delta;
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
    // Match HarfBuzz's retained CFF2/VARC axes. VARC instancing is rejected by
    // the caller; HarfBuzz cannot currently partially pin CFF2 blends.
    let detect_pins = font.cff2().is_err() && font.data_for_tag(Tag::new(b"VARC")).is_none();
    let avar = font.avar().map_err(|_| error())?;
    let store = avar.var_store().ok_or_else(error)?.map_err(|_| error())?;
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
            let value = (intermediate as f32 * 16384.).round() as i32
                + (min as f32).round().clamp(-32768., 32768.) as i32;
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
) -> Result<Avar, SubsetError> {
    let old = font.avar().map_err(|_| error())?;
    let store = old.var_store().ok_or_else(error)?.map_err(|_| error())?;
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
        let default_delta = store
            .compute_delta(index, &inputs.coords)
            .map_or(0., |v| v.to_f64().round());
        let u = axes.user[i];
        let original = (
            axis.min_value().to_f64(),
            axis.default_value().to_f64(),
            axis.max_value().to_f64(),
        );
        let restricted = (u.0, u.1, u.2) != original || axes.user_pinned[i];
        let middle = if restricted { axes.normalized[i].1 } else { 0. };
        add(&mut row, neutral.clone(), middle * 16384. + default_delta);

        if restricted && !axes.user_pinned[i] {
            let t = axes.normalized[i];
            let bytes = write_fonts::dump_table(&segment_maps[new_index]).map_err(|_| error())?;
            let new_map =
                write_fonts::read::tables::avar::SegmentMaps::read(bytes.as_slice().into())
                    .map_err(|_| error())?;
            let mut knots = BTreeMap::new();
            for (z, offset) in [(-1., t.0 + 1.), (0., t.1), (1., t.2 - 1.)] {
                knots.insert(F2Dot14::from_f64(z), offset);
            }
            let z = axis_to_normalized(original.1, u.0, u.1, u.2);
            if z > -1. && z < 1. && z != 0. {
                let z = F2Dot14::from_f64(map_float(&new_map, z, false));
                if z != F2Dot14::ZERO {
                    knots.insert(z, -z.to_f64());
                }
            }
            // New segment-map kinks can move with rounding. Put matching
            // compensation knots there to avoid joining across a kink.
            for value in new_map.axis_value_maps() {
                let from = value.from_coordinate().to_f64();
                let z = value.to_coordinate();
                if !(-1. < from && from < 1.) || z == F2Dot14::ZERO {
                    continue;
                }
                let user = u.1 + from * if from < 0. { u.1 - u.0 } else { u.2 - u.1 };
                let n =
                    F2Dot14::from_f64(axis_to_normalized(user, original.0, original.1, original.2));
                let x = F2Dot14::from_f64(map_float(
                    old_maps.get(i).ok_or_else(error)?,
                    n.to_f64(),
                    false,
                ));
                knots.insert(z, x.to_f64() - z.to_f64());
            }
            let knots: Vec<_> = knots.into_iter().collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use skrifa::MetadataProvider;
    use write_fonts::{
        from_obj::{FromTableRef, ToOwnedTable},
        FontBuilder,
    };

    fn font(shared: bool, implicit: bool) -> Vec<u8> {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let mut avar: Avar = font.avar().unwrap().to_owned_table();
        let region = |values: [(f64, f64, f64); 2]| VariationRegion {
            region_axes: values
                .into_iter()
                .map(|(a, b, c)| {
                    RegionAxisCoordinates::new(
                        F2Dot14::from_f64(a),
                        F2Dot14::from_f64(b),
                        F2Dot14::from_f64(c),
                    )
                })
                .collect(),
        };
        avar.var_store = Some(ItemVariationStore {
            variation_region_list: VariationRegionList {
                axis_count: 2,
                variation_regions: vec![
                    region([(0., 0., 0.), (0., 1., 1.)]),
                    region([(0., 0.5, 1.), (0., 0., 0.)]),
                    region([(-1., -1., 0.), (0., 1., 1.)]),
                ],
            }
            .into(),
            item_variation_data: vec![ItemVariationData {
                item_count: 2,
                word_delta_count: 0x8003,
                region_indexes: vec![0, 1, 2],
                delta_sets: [512i32, 256, -128, -256, 128, 64]
                    .into_iter()
                    .flat_map(i32::to_be_bytes)
                    .collect(),
            }
            .into()],
        })
        .into();
        if !implicit {
            avar.axis_index_map = Some(
                [0u32, if shared { 0 } else { 1 }]
                    .into_iter()
                    .collect::<DeltaSetIndexMap>(),
            )
            .into();
        }
        let mut builder = FontBuilder::new();
        for record in font.table_directory().table_records() {
            if record.tag() != Tag::new(b"avar") {
                builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
            }
        }
        builder.add_table(&avar).unwrap();
        builder.build()
    }

    fn truetype_font(cancel: bool) -> Vec<u8> {
        let bytes = std::fs::read("test-data/fonts/RobotoFlex-Variable.ABC.ttf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let count = font.axes().len();
        let region = |axis: usize| VariationRegion {
            region_axes: (0..count)
                .map(|i| {
                    RegionAxisCoordinates::new(
                        F2Dot14::ZERO,
                        if i == axis {
                            F2Dot14::ONE
                        } else {
                            F2Dot14::ZERO
                        },
                        if i == axis {
                            F2Dot14::ONE
                        } else {
                            F2Dot14::ZERO
                        },
                    )
                })
                .collect(),
        };
        let mut rows = vec![0i32; count * 3];
        // The width contributions to the weight row cancel exactly. Checking
        // each region independently would fail to recognize this constant pin.
        rows[..3].copy_from_slice(&[1024, 512, if cancel { -512 } else { -256 }]);
        rows[3] = 256; // The retained width axis receives a weight contribution.
        rows[7] = -128; // Optical size depends on width, after the dropped axis.
        let mut avar = Avar::new(
            (0..count)
                .map(|_| {
                    SegmentMaps::new(
                        [-1., 0., 1.]
                            .into_iter()
                            .map(|v| {
                                write_fonts::tables::avar::AxisValueMap::new(
                                    F2Dot14::from_f64(v),
                                    F2Dot14::from_f64(v),
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        );
        avar.var_store = Some(ItemVariationStore {
            variation_region_list: VariationRegionList {
                axis_count: count as u16,
                variation_regions: vec![region(0), region(1), region(1)],
            }
            .into(),
            item_variation_data: vec![ItemVariationData {
                item_count: count as u16,
                word_delta_count: 0x8003,
                region_indexes: vec![0, 1, 2],
                delta_sets: rows.into_iter().flat_map(i32::to_be_bytes).collect(),
            }
            .into()],
        })
        .into();
        let mut builder = FontBuilder::new();
        for record in font.table_directory().table_records() {
            if record.tag() != Tag::new(b"avar") {
                builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
            }
        }
        builder.add_table(&avar).unwrap();
        builder.build()
    }

    #[test]
    fn dependent_pins_cull_gvar_like_harfbuzz() {
        let bytes = truetype_font(false);
        let original = FontRef::new(&bytes).unwrap();
        let out = crate::instance_font(&original, &crate::parse_axis_limits("wght=700").unwrap())
            .unwrap();
        let actual = FontRef::new(&out).unwrap();
        let reference = include_bytes!("../../test-data/expected/avar2-pins/dependent-pruned.ttf");
        let expected = FontRef::new(reference).unwrap();
        let actual = actual.gvar().unwrap();
        let expected = expected.gvar().unwrap();
        let coords = |t: write_fonts::read::tables::variations::Tuple<'_>| {
            (0..t.len()).map(|i| t.get(i).unwrap()).collect::<Vec<_>>()
        };
        let tuples = |gvar: &write_fonts::read::tables::gvar::Gvar<'_>, gid| {
            gvar.glyph_variation_data(gid)
                .unwrap()
                .map_or_else(Vec::new, |data| {
                    data.tuples()
                        .map(|t| {
                            (
                                coords(t.peak()),
                                t.intermediate_start().map(coords),
                                t.intermediate_end().map(coords),
                                t.deltas().collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
        };
        let mut count = 0;
        for gid in 0..actual.glyph_count() {
            let gid = write_fonts::types::GlyphId::new(gid as u32);
            let actual = tuples(&actual, gid);
            let expected = tuples(&expected, gid);
            count += actual.len();
            assert_eq!(actual, expected, "{gid:?}");
        }
        assert_eq!(count, 149);
    }

    #[test]
    fn reachable_ranges_cull_all_final_space_stores_without_changing_indices() {
        use write_fonts::tables::{
            base::Base, colr::Colr, gdef::Gdef, hvar::Hvar, mvar::Mvar, vvar::Vvar,
        };
        let bytes = truetype_font(false);
        let original = FontRef::new(&bytes).unwrap();
        let axes = original.axes().len();
        let region = |negative| {
            VariationRegion::new(
                (0..axes)
                    .map(|i| {
                        let (a, b, c) = if i != 0 {
                            (0., 0., 0.)
                        } else if negative {
                            (-1., -1., 0.)
                        } else {
                            (0., 1., 1.)
                        };
                        RegionAxisCoordinates::new(
                            F2Dot14::from_f64(a),
                            F2Dot14::from_f64(b),
                            F2Dot14::from_f64(c),
                        )
                    })
                    .collect(),
            )
        };
        let store = ItemVariationStore::new(
            VariationRegionList::new(axes as u16, vec![region(true), region(false)]),
            vec![Some(ItemVariationData {
                item_count: 4,
                word_delta_count: 2,
                region_indexes: vec![0, 1],
                delta_sets: [100i16, 200, -100, -200, 300, 400, -300, -400]
                    .into_iter()
                    .flat_map(i16::to_be_bytes)
                    .collect(),
            })],
        );
        let mut builder = FontBuilder::new();
        for r in original.table_directory().table_records() {
            if r.tag() != Tag::new(b"GPOS") {
                builder.add_raw(r.tag(), original.data_for_tag(r.tag()).unwrap());
            }
        }
        builder.add_raw(
            Tag::new(b"vhea"),
            original.data_for_tag(Tag::new(b"hhea")).unwrap(),
        );
        builder.add_raw(
            Tag::new(b"vmtx"),
            original.data_for_tag(Tag::new(b"hmtx")).unwrap(),
        );
        builder
            .add_table(&Hvar::new(store.clone(), None, None, None))
            .unwrap();
        builder
            .add_table(&Vvar::new(store.clone(), None, None, None, None))
            .unwrap();
        builder
            .add_table(&Mvar {
                version: write_fonts::types::MajorMinor::VERSION_1_0,
                value_record_size: 8,
                value_record_count: 1,
                value_records: vec![write_fonts::tables::mvar::ValueRecord::new(
                    Tag::new(b"hasc"),
                    0,
                    0,
                )],
                item_variation_store: Some(store.clone()).into(),
            })
            .unwrap();
        builder
            .add_table(&Gdef {
                item_var_store: Some(store.clone()).into(),
                ..Default::default()
            })
            .unwrap();
        builder
            .add_table(&Base {
                item_var_store: Some(store.clone()).into(),
                ..Default::default()
            })
            .unwrap();
        builder
            .add_table(&Colr {
                item_variation_store: Some(store).into(),
                ..Default::default()
            })
            .unwrap();
        let source = builder.build();
        let source = FontRef::new(&source).unwrap();
        let out =
            crate::instance_font(&source, &crate::parse_axis_limits("wght=700").unwrap()).unwrap();
        let out = FontRef::new(&out).unwrap();
        let stores = [
            out.hvar().unwrap().item_variation_store().unwrap(),
            out.vvar().unwrap().item_variation_store().unwrap(),
            out.mvar().unwrap().item_variation_store().unwrap().unwrap(),
            out.gdef().unwrap().item_var_store().unwrap().unwrap(),
            out.base().unwrap().item_var_store().unwrap().unwrap(),
            out.colr().unwrap().item_variation_store().unwrap().unwrap(),
        ];
        for store in stores {
            assert_eq!(store.variation_region_list().unwrap().region_count(), 1);
            let data = store.item_variation_data().get(0).unwrap().unwrap();
            assert_eq!(data.item_count(), 4);
            assert_eq!(data.delta_set(0).collect::<Vec<_>>(), vec![200]);
            assert_eq!(data.delta_set(1).collect::<Vec<_>>(), vec![-200]);
        }
        // The avar2 transform's regions live in intermediate space, so its
        // width-dependent weight row is retained rather than culled here.
        let avar = out.avar().unwrap();
        let store = avar.var_store().unwrap().unwrap();
        let map = avar.axis_index_map().unwrap().unwrap();
        let index = map.get(0).unwrap();
        let data = store
            .item_variation_data()
            .get(index.outer as usize)
            .unwrap()
            .unwrap();
        assert!(data.delta_set(index.inner).any(|delta| delta != 0));
    }

    #[test]
    fn oversized_grids_conservatively_retain_dependent_pins() {
        let bytes = truetype_font(true);
        let original = FontRef::new(&bytes).unwrap();
        let mut fvar: write_fonts::tables::fvar::Fvar = original.fvar().unwrap().to_owned_table();
        for axis in &mut fvar.axis_instance_arrays.axes {
            axis.flags = 0;
        }
        let mut avar: Avar = original.avar().unwrap().to_owned_table();
        let store = avar.var_store.as_mut().unwrap();
        let region = VariationRegion {
            region_axes: (0..original.axes().len())
                .map(|_| {
                    RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::from_f64(0.5), F2Dot14::ONE)
                })
                .collect(),
        };
        store
            .variation_region_list
            .variation_regions
            .extend([region.clone(), region]);
        let data = store.item_variation_data[0].as_mut().unwrap();
        let old = data.delta_sets.clone();
        data.delta_sets.clear();
        for (i, row) in old.chunks_exact(12).enumerate() {
            data.delta_sets.extend(row);
            for delta in if i == 0 { [512i32, -512] } else { [0, 0] } {
                data.delta_sets.extend(delta.to_be_bytes());
            }
        }
        data.region_indexes.extend([3, 4]);
        data.word_delta_count = 0x8005;
        let mut builder = FontBuilder::new();
        for record in original.table_directory().table_records() {
            if ![Tag::new(b"fvar"), Tag::new(b"avar")].contains(&record.tag()) {
                builder.add_raw(record.tag(), original.data_for_tag(record.tag()).unwrap());
            }
        }
        builder.add_table(&fvar).unwrap();
        builder.add_table(&avar).unwrap();
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=700").unwrap()).unwrap();
        // Twelve free axes exceed the 16384-vertex grid limit. Interval
        // bounds safely retain the pin even though these extra rows cancel.
        assert!(!axes.pinned[0]);
    }

    #[test]
    fn missing_axis_delta_rows_have_constant_zero_offsets() {
        let bytes = truetype_font(true);
        let original = FontRef::new(&bytes).unwrap();
        let mut avar: Avar = original.avar().unwrap().to_owned_table();
        avar.axis_index_map = Some(
            (0..original.axes().len() as u32)
                .map(|i| if i == 0 { u32::MAX } else { i })
                .collect::<DeltaSetIndexMap>(),
        )
        .into();
        let mut builder = FontBuilder::new();
        for record in original.table_directory().table_records() {
            if record.tag() != Tag::new(b"avar") {
                builder.add_raw(record.tag(), original.data_for_tag(record.tag()).unwrap());
            }
        }
        builder.add_table(&avar).unwrap();
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=700").unwrap()).unwrap();
        assert!(axes.pinned[0]);
        assert_eq!(axes.coords[0], F2Dot14::from_f64(0.5));
    }

    #[test]
    fn self_contained_pin_matches_harfbuzz_geometry_metrics_and_layout() {
        use skrifa::{
            instance::Size,
            outline::{DrawSettings, OutlinePen},
        };
        use write_fonts::types::GlyphId;
        #[derive(Default, Debug, PartialEq)]
        struct Path(Vec<(u8, Vec<f32>)>);
        impl OutlinePen for Path {
            fn move_to(&mut self, x: f32, y: f32) {
                self.0.push((0, vec![x, y]));
            }
            fn line_to(&mut self, x: f32, y: f32) {
                self.0.push((1, vec![x, y]));
            }
            fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
                self.0.push((2, vec![cx, cy, x, y]));
            }
            fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
                self.0.push((3, vec![ax, ay, bx, by, x, y]));
            }
            fn close(&mut self) {
                self.0.push((4, vec![]));
            }
        }
        let bytes = truetype_font(true);
        let original = FontRef::new(&bytes).unwrap();
        let out = crate::instance_font(&original, &crate::parse_axis_limits("wght=700").unwrap())
            .unwrap();
        let actual = FontRef::new(&out).unwrap();
        let reference =
            std::fs::read("test-data/expected/avar2-pins/self-contained-pin.ttf").unwrap();
        let expected = FontRef::new(&reference).unwrap();
        assert_eq!(
            write_fonts::tables::fvar::Fvar::from_table_ref(&actual.fvar().unwrap()),
            write_fonts::tables::fvar::Fvar::from_table_ref(&expected.fvar().unwrap())
        );
        assert_eq!(
            write_fonts::tables::gdef::Gdef::from_table_ref(&actual.gdef().unwrap()),
            write_fonts::tables::gdef::Gdef::from_table_ref(&expected.gdef().unwrap())
        );
        assert_eq!(
            write_fonts::tables::gpos::Gpos::from_table_ref(&actual.gpos().unwrap()),
            write_fonts::tables::gpos::Gpos::from_table_ref(&expected.gpos().unwrap())
        );
        assert_eq!(
            write_fonts::tables::gsub::Gsub::from_table_ref(&actual.gsub().unwrap()),
            write_fonts::tables::gsub::Gsub::from_table_ref(&expected.gsub().unwrap())
        );
        let ao = actual.outline_glyphs();
        let eo = expected.outline_glyphs();
        // A pin outside a tuple's support must remove that tuple rather than
        // retaining a zero-scaled residual on the other axes.
        let tuple_count = |font: &FontRef| {
            let gvar = font.gvar().unwrap();
            (0..gvar.glyph_count())
                .map(|gid| {
                    gvar.glyph_variation_data(write_fonts::types::GlyphId::new(gid as u32))
                        .unwrap()
                        .map_or(0, |data| data.tuples().count())
                })
                .sum::<usize>()
        };
        assert_eq!(tuple_count(&actual), 98);
        assert_eq!(tuple_count(&actual), tuple_count(&expected));
        for width in [25., 62.5, 100., 125.5, 151.] {
            for size in [8., 14., 76., 144.] {
                let settings = [(Tag::new(b"wdth"), width), (Tag::new(b"opsz"), size)];
                let al = actual.axes().location(settings);
                let el = expected.axes().location(settings);
                assert_eq!(al.coords(), el.coords());
                let am = actual.glyph_metrics(Size::unscaled(), &al);
                let em = expected.glyph_metrics(Size::unscaled(), &el);
                for gid in 0..actual.maxp().unwrap().num_glyphs() {
                    let gid = GlyphId::new(gid as u32);
                    let mut a = Path::default();
                    let mut e = Path::default();
                    ao.get(gid)
                        .unwrap()
                        .draw(DrawSettings::unhinted(Size::unscaled(), &al), &mut a)
                        .unwrap();
                    eo.get(gid)
                        .unwrap()
                        .draw(DrawSettings::unhinted(Size::unscaled(), &el), &mut e)
                        .unwrap();
                    assert_eq!(a, e, "width={width} size={size} {gid:?}");
                    assert_eq!(am.advance_width(gid), em.advance_width(gid));
                    assert_eq!(am.left_side_bearing(gid), em.left_side_bearing(gid));
                }
            }
        }
    }

    #[test]
    fn constant_final_pins_are_dropped_but_dependent_pins_stay_hidden() {
        for cancel in [true, false] {
            let bytes = truetype_font(cancel);
            let original = FontRef::new(&bytes).unwrap();
            let limits = crate::parse_axis_limits("wght=700").unwrap();
            let out = crate::instance_font(&original, &limits).unwrap();
            let partial = FontRef::new(&out).unwrap();
            assert_eq!(
                partial.axes().len(),
                original.axes().len() - usize::from(cancel)
            );
            assert_eq!(
                partial.gvar().unwrap().axis_count() as usize,
                partial.axes().len()
            );
            let pin = partial.axes().get_by_tag(Tag::new(b"wght"));
            if cancel {
                assert!(pin.is_none());
            } else {
                let fvar = partial.fvar().unwrap();
                let axes = fvar.axes().unwrap();
                assert_eq!(axes[0].flags() & 1, 1);
                assert_eq!(axes[0].min_value(), axes[0].max_value());
            }
            for width in [25., 62.5, 100., 125.5, 151.] {
                let old = original
                    .axes()
                    .location([(Tag::new(b"wght"), 700.), (Tag::new(b"wdth"), width)]);
                let new = partial.axes().location([(Tag::new(b"wdth"), width)]);
                let retained: Vec<_> = old
                    .coords()
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !cancel || *i != 0)
                    .map(|(_, c)| *c)
                    .collect();
                assert_eq!(retained, new.coords(), "cancel={cancel} width={width}");
            }
            // Completing at the retained defaults preserves the direct full
            // instance, including the pin's nonzero final-space offset.
            let all: Vec<_> = original
                .axes()
                .iter()
                .map(|a| crate::AxisLimits::Pin {
                    tag: a.tag(),
                    value: if a.tag() == Tag::new(b"wght") {
                        700.
                    } else {
                        a.default_value()
                    },
                })
                .collect();
            let direct = crate::instance_font(&original, &all).unwrap();
            let remaining: Vec<_> = partial
                .axes()
                .iter()
                .map(|a| crate::AxisLimits::Pin {
                    tag: a.tag(),
                    value: a.default_value(),
                })
                .collect();
            let composed = crate::instance_font(&partial, &remaining).unwrap();
            let direct = FontRef::new(&direct).unwrap();
            let composed = FontRef::new(&composed).unwrap();
            let dg = direct.glyf().unwrap();
            let cg = composed.glyf().unwrap();
            let dl = direct.loca(None).unwrap();
            let cl = composed.loca(None).unwrap();
            for gid in 0..direct.maxp().unwrap().num_glyphs() {
                let gid = write_fonts::types::GlyphId::new(gid as u32);
                let d = dl.get(gid, &dg).unwrap().into_glyph();
                let c = cl.get(gid, &cg).unwrap().into_glyph();
                let (d, c) = match (d, c) {
                    (None, None) => continue,
                    (
                        Some(write_fonts::read::tables::glyf::Glyph::Simple(d)),
                        Some(write_fonts::read::tables::glyf::Glyph::Simple(c)),
                    ) => (d, c),
                    _ => panic!("different glyph types"),
                };
                assert_eq!(d.num_points(), c.num_points());
                // Staging rounds the baked outline and residual tuples
                // separately. Compare against the direct instance within
                // one design unit; reference tests check exact HB geometry.
                for (d, c) in d.points().zip(c.points()) {
                    assert!((d.x as i32 - c.x as i32).abs() <= 1);
                    assert!((d.y as i32 - c.y as i32).abs() <= 1);
                }
                assert!(
                    (direct.hmtx().unwrap().advance(gid).unwrap() as i32
                        - composed.hmtx().unwrap().advance(gid).unwrap() as i32)
                        .abs()
                        <= 1
                );
            }
        }
    }

    #[test]
    fn coupled_pins_ranges_and_shared_rows_preserve_final_coordinates() {
        for (shared, implicit) in [(false, false), (true, false), (false, true)] {
            let bytes = font(shared, implicit);
            let original = FontRef::new(&bytes).unwrap();
            for request in [
                "wght=700",
                "CNTR=40",
                "wght=300:550:700,CNTR=25:75",
                "wght=200:650:900",
                "CNTR=25:60:75",
                "wght=900",
            ] {
                let limits = crate::parse_axis_limits(request).unwrap();
                let out = crate::instance_font(&original, &limits).unwrap();
                let partial = FontRef::new(&out).unwrap();
                assert_eq!(partial.axes().len(), original.axes().len());
                if limits.iter().any(|l| l.tag() == Tag::new(b"wght")) {
                    let weight = partial
                        .axes()
                        .get_by_tag(Tag::new(b"wght"))
                        .unwrap()
                        .default_value();
                    assert_eq!(
                        partial.os2().unwrap().us_weight_class(),
                        weight.round() as u16
                    );
                }
                // All consumers still interpret the original final coordinates.
                for tag in [b"CFF2", b"GPOS"] {
                    assert_eq!(
                        original.data_for_tag(Tag::new(tag)).map(|d| d.as_bytes()),
                        partial.data_for_tag(Tag::new(tag)).map(|d| d.as_bytes())
                    );
                }
                for a in 0..=12 {
                    for b in 0..=12 {
                        let settings: Vec<_> = partial
                            .axes()
                            .iter()
                            .zip([a, b])
                            .map(|(axis, i)| {
                                (
                                    axis.tag(),
                                    axis.min_value()
                                        + (axis.max_value() - axis.min_value()) * i as f32 / 12.,
                                )
                            })
                            .collect();
                        let old = original.axes().location(settings.iter().copied());
                        let new = partial.axes().location(settings.iter().copied());
                        for (x, y) in old.coords().iter().zip(new.coords()) {
                            assert!((x.to_bits() as i32 - y.to_bits() as i32).abs() <= 8,
                                "{request} shared={shared} implicit={implicit} {settings:?}: {:?} != {:?}", old.coords(),new.coords());
                        }
                        // Culling may rewrite stores, but retained final-space
                        // locations still evaluate to the same metric deltas.
                        let old_metrics =
                            original.glyph_metrics(skrifa::instance::Size::unscaled(), &new);
                        let new_metrics =
                            partial.glyph_metrics(skrifa::instance::Size::unscaled(), &new);
                        for gid in 0..partial.maxp().unwrap().num_glyphs() {
                            let gid = write_fonts::types::GlyphId::new(gid as u32);
                            assert_eq!(
                                old_metrics.advance_width(gid),
                                new_metrics.advance_width(gid)
                            );
                        }
                    }
                }
                for limit in limits {
                    if let crate::AxisLimits::Pin { tag, .. } = limit {
                        let fvar = partial.fvar().unwrap();
                        let arrays = fvar.axis_instance_arrays().unwrap();
                        let axis = arrays.axes().iter().find(|a| a.axis_tag() == tag).unwrap();
                        assert_eq!(axis.flags() & 1, 1);
                        assert_eq!(axis.min_value(), axis.max_value());
                    }
                }
            }
        }
    }

    #[test]
    fn coupled_width_pins_update_style_metadata() {
        let bytes = font(false, false);
        let original = FontRef::new(&bytes).unwrap();
        let mut fvar: write_fonts::tables::fvar::Fvar = original.fvar().unwrap().to_owned_table();
        let width = &mut fvar.axis_instance_arrays.axes[1];
        width.axis_tag = Tag::new(b"wdth");
        width.min_value = write_fonts::types::Fixed::from_f64(50.);
        width.default_value = write_fonts::types::Fixed::from_f64(100.);
        width.max_value = write_fonts::types::Fixed::from_f64(200.);
        fvar.axis_instance_arrays.instances.clear();
        let mut builder = FontBuilder::new();
        for record in original.table_directory().table_records() {
            if record.tag() != Tag::new(b"fvar") {
                builder.add_raw(record.tag(), original.data_for_tag(record.tag()).unwrap());
            }
        }
        builder.add_table(&fvar).unwrap();
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let bytes =
            crate::instance_font(&font, &crate::parse_axis_limits("wdth=125").unwrap()).unwrap();
        assert_eq!(
            FontRef::new(&bytes)
                .unwrap()
                .os2()
                .unwrap()
                .us_width_class(),
            7
        );
    }

    #[test]
    fn coupled_partial_then_full_instance_resolves_hidden_axes() {
        use write_fonts::read::{model::glyph::outline::PathElement, ps::cff::CffFontRef};
        use write_fonts::types::GlyphId;
        let bytes = font(true, false);
        let original = FontRef::new(&bytes).unwrap();
        let partial =
            crate::instance_font(&original, &crate::parse_axis_limits("wght=900").unwrap())
                .unwrap();
        let partial = FontRef::new(&partial).unwrap();
        let composed =
            crate::instance_font(&partial, &crate::parse_axis_limits("CNTR=50").unwrap()).unwrap();
        let direct = crate::instance_font(
            &original,
            &crate::parse_axis_limits("wght=900,CNTR=50").unwrap(),
        )
        .unwrap();
        let composed = FontRef::new(&composed).unwrap();
        let direct = FontRef::new(&direct).unwrap();
        assert!(composed.fvar().is_err());
        assert!(composed.avar().is_err());
        assert_eq!(
            composed.os2().unwrap().us_weight_class(),
            direct.os2().unwrap().us_weight_class()
        );
        let a = CffFontRef::new(
            composed.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
            0,
            None,
        )
        .unwrap();
        let b = CffFontRef::new(
            direct.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
            0,
            None,
        )
        .unwrap();
        for gid in 0..a.num_glyphs() {
            let gid = GlyphId::new(gid);
            let sa = a.subfont(a.subfont_index(gid).unwrap(), &[]).unwrap();
            let sb = b.subfont(b.subfont_index(gid).unwrap(), &[]).unwrap();
            let mut pa = Vec::<PathElement>::new();
            let mut pb = Vec::<PathElement>::new();
            a.draw(&sa, gid, &[], None, &mut pa).unwrap();
            b.draw(&sb, gid, &[], None, &mut pb).unwrap();
            assert_eq!(pa, pb);
        }
        assert_eq!(
            composed.data_for_tag(Tag::new(b"hmtx")).unwrap().as_bytes(),
            direct.data_for_tag(Tag::new(b"hmtx")).unwrap().as_bytes()
        );
    }
}
