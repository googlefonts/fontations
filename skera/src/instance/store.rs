//! A shared linear transform for CFF2 blends and ordinary item delta rows.
use super::{
    rebase::{self, Triple},
    AxisPlan,
};
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    read::tables::variations::ItemVariationStore,
    tables::variations as owned,
    types::{F2Dot14, Tag},
};

fn error() -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(b"CFF2"))
}

/// Repeated region indexes let rebased sums exceed a single LONG_WORD delta.
pub(super) fn encode_rows(
    indices: &[u16],
    rows: &[Vec<f64>],
) -> Result<owned::ItemVariationData, SubsetError> {
    let mut copies = vec![1usize; indices.len()];
    for row in rows {
        if row.len() != indices.len() {
            return Err(error());
        }
        for (copies, &value) in copies.iter_mut().zip(row) {
            let value = value.round();
            if !value.is_finite() || value.abs() > (1u64 << 48) as f64 {
                return Err(error());
            }
            let limit = if value < 0. {
                -(i32::MIN as f64)
            } else {
                i32::MAX as f64
            };
            *copies = (*copies).max((value.abs() / limit).ceil() as usize);
        }
    }
    let count = copies.iter().try_fold(0usize, |count, &n| {
        let count = count.checked_add(n).ok_or_else(error)?;
        if count > 0x7fff {
            Err(error())
        } else {
            Ok(count)
        }
    })?;
    let mut delta_sets = Vec::new();
    for row in rows {
        for (&value, &copies) in row.iter().zip(&copies) {
            let mut value = value.round() as i64;
            for _ in 0..copies {
                let part = value.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                delta_sets.extend(part.to_be_bytes());
                value -= part as i64;
            }
        }
    }
    Ok(owned::ItemVariationData {
        item_count: u16::try_from(rows.len()).map_err(|_| error())?,
        word_delta_count: 0x8000 | count as u16,
        region_indexes: indices
            .iter()
            .zip(copies)
            .flat_map(|(&i, n)| std::iter::repeat_n(i, n))
            .collect(),
        delta_sets,
    })
}

type Region = Vec<(i16, i16, i16)>;
pub(crate) struct Transform {
    pub gains: Vec<f64>,
    pub weights: Vec<Vec<f64>>,
    pub indices: Vec<u16>,
}
impl Transform {
    pub fn residual(&self, deltas: &[f64]) -> Result<Vec<f64>, SubsetError> {
        if deltas.len() != self.gains.len() {
            return Err(error());
        }
        Ok(self
            .weights
            .iter()
            .map(|row| row.iter().zip(deltas).map(|(g, d)| g * d).sum())
            .collect())
    }
}
pub(crate) struct StorePlan {
    pub transforms: Vec<Transform>,
    regions: Vec<Region>,
    axis_count: u16,
}
impl StorePlan {
    pub fn new(store: &ItemVariationStore, axes: &AxisPlan) -> Result<Self, SubsetError> {
        let region_list = store.variation_region_list().map_err(|_| error())?;
        if region_list.axis_count() as usize != axes.pinned.len() {
            return Err(error());
        }
        let mut expanded = Vec::new();
        let mut regions = Vec::new();
        let mut region_map = BTreeMap::new();
        for region in region_list.variation_regions().iter() {
            let region = region.map_err(|_| error())?;
            let mut terms = vec![(1., Vec::new())];
            for (i, t) in region.region_axes().iter().enumerate() {
                let t = Triple(
                    t.start_coord().to_f64(),
                    t.peak_coord().to_f64(),
                    t.end_coord().to_f64(),
                );
                let rebased = if axes.pinned[i] {
                    vec![(rebase::scalar(axes.coords[i].to_f64(), t), None)]
                } else {
                    rebase::rebase(t, axes.normalized[i], axes.distances[i])
                };
                if terms.len().saturating_mul(rebased.len()) > 65535 {
                    return Err(error());
                }
                let mut next = Vec::new();
                for (gain, region) in terms {
                    for &(g, t) in &rebased {
                        if gain * g == 0. {
                            continue;
                        }
                        let mut r = region.clone();
                        if !axes.pinned[i] {
                            let t = t.unwrap_or(Triple(0., 0., 0.));
                            r.push((
                                F2Dot14::from_f64(t.0.clamp(-2., 1.99993896484375)).to_bits(),
                                F2Dot14::from_f64(t.1.clamp(-2., 1.99993896484375)).to_bits(),
                                F2Dot14::from_f64(t.2.clamp(-2., 1.99993896484375)).to_bits(),
                            ));
                        }
                        next.push((gain * g, r));
                    }
                }
                terms = next;
            }
            let mut gain = 0.;
            let mut residual = BTreeMap::new();
            for (g, r) in terms {
                if r.iter().all(|t| t.1 == 0) {
                    gain += g;
                    continue;
                }
                let idx = if let Some(&idx) = region_map.get(&r) {
                    idx
                } else {
                    let idx = u16::try_from(regions.len()).map_err(|_| error())?;
                    region_map.insert(r.clone(), idx);
                    regions.push(r);
                    idx
                };
                *residual.entry(idx).or_insert(0.) += g;
            }
            expanded.push((gain, residual));
        }
        let mut transforms = Vec::new();
        for data in store.item_variation_data().iter() {
            let Some(data) = data else {
                transforms.push(Transform {
                    gains: vec![],
                    weights: vec![],
                    indices: vec![],
                });
                continue;
            };
            let data = data.map_err(|_| error())?;
            let mut gains = Vec::new();
            let mut columns: BTreeMap<u16, Vec<f64>> = BTreeMap::new();
            let k = data.region_index_count() as usize;
            for (i, idx) in data.region_indexes().iter().enumerate() {
                let (gain, terms) = expanded.get(idx.get() as usize).ok_or_else(error)?;
                gains.push(*gain);
                for (&idx, &g) in terms {
                    if !columns.contains_key(&idx) && columns.len().saturating_mul(k) > 4_000_000 {
                        return Err(error());
                    }
                    columns.entry(idx).or_insert_with(|| vec![0.; k])[i] += g;
                }
            }
            let (indices, weights) = columns.into_iter().unzip();
            transforms.push(Transform {
                gains,
                indices,
                weights,
            });
        }
        Ok(Self {
            transforms,
            regions,
            axis_count: axes.pinned.iter().filter(|&&p| !p).count() as u16,
        })
    }
    pub fn rebuild(
        &self,
        store: &ItemVariationStore,
    ) -> Result<owned::ItemVariationStore, SubsetError> {
        let mut data = Vec::new();
        for (i, old) in store.item_variation_data().iter().enumerate() {
            let Some(old) = old else {
                data.push(Default::default());
                continue;
            };
            let old = old.map_err(|_| error())?;
            let t = &self.transforms[i];
            let mut rows = Vec::new();
            for inner in 0..old.item_count() {
                let deltas: Vec<_> = old.delta_set(inner).map(|d| d as f64).collect();
                rows.push(t.residual(&deltas)?);
            }
            data.push(encode_rows(&t.indices, &rows)?.into());
        }
        Ok(owned::ItemVariationStore {
            variation_region_list: owned::VariationRegionList {
                axis_count: self.axis_count,
                variation_regions: self
                    .regions
                    .iter()
                    .map(|r| owned::VariationRegion {
                        region_axes: r
                            .iter()
                            .map(|&(a, b, c)| {
                                owned::RegionAxisCoordinates::new(
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::read::{FontData, FontRead, FontRef};

    #[test]
    fn merged_long_word_columns_preserve_large_delta_sums() {
        use write_fonts::tables::variations::*;
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let axes = AxisPlan::new(&font, &crate::parse_axis_limits("CNTR=drop").unwrap()).unwrap();
        let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
        let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
        let region = VariationRegion::new(vec![pos, zero]);
        let bytes = write_fonts::dump_table(&write_fonts::tables::variations::ItemVariationStore {
            variation_region_list: VariationRegionList::new(2, vec![region.clone(), region]).into(),
            item_variation_data: vec![ItemVariationData {
                item_count: 2,
                word_delta_count: 0x8002,
                region_indexes: vec![0, 1],
                delta_sets: [i32::MAX, i32::MAX, i32::MIN, i32::MIN]
                    .into_iter()
                    .flat_map(i32::to_be_bytes)
                    .collect(),
            }
            .into()],
        })
        .unwrap();
        let original =
            write_fonts::read::tables::variations::ItemVariationStore::read(FontData::new(&bytes))
                .unwrap();
        let plan = StorePlan::new(&original, &axes).unwrap();
        let output = write_fonts::dump_table(&plan.rebuild(&original).unwrap()).unwrap();
        let output =
            write_fonts::read::tables::variations::ItemVariationStore::read(FontData::new(&output))
                .unwrap();
        for inner in 0..2 {
            for coord in [F2Dot14::ZERO, F2Dot14::from_f64(0.5), F2Dot14::ONE] {
                let index =
                    write_fonts::read::tables::variations::DeltaSetIndex { outer: 0, inner };
                assert_eq!(
                    original.compute_delta(index, &[coord, F2Dot14::ZERO]),
                    output.compute_delta(index, &[coord])
                );
            }
        }
    }
}
