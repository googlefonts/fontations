// Copyright © 2017 Google, Inc.
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
    let count = axes.pinned.len();
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
                .map(|pinned| {
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
            let bytes = write_fonts::dump_table(&segment_maps[i]).map_err(|_| error())?;
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
                region[i] = (
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
    use write_fonts::{from_obj::ToOwnedTable, FontBuilder};

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
                for tag in [b"CFF2", b"HVAR", b"GPOS", b"GDEF"] {
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
