use super::rebase::{renormalize, Triple};
use crate::SubsetError;
use skrifa::MetadataProvider;
use write_fonts::{
    read::{FontRef, TableProvider},
    types::{F2Dot14, Tag},
};

/// An axis pin or retained user-coordinate range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisLimits {
    /// Remove an axis at the specified user coordinate.
    Pin { tag: Tag, value: f32 },
    /// Remove an axis at its original default.
    Drop { tag: Tag },
    /// Retain an axis with new bounds and an optional new default.
    Range {
        tag: Tag,
        min: f32,
        default: Option<f32>,
        max: f32,
    },
}
impl AxisLimits {
    pub(crate) fn tag(self) -> Tag {
        match self {
            Self::Pin { tag, .. } | Self::Drop { tag } | Self::Range { tag, .. } => tag,
        }
    }
}

/// Parse `wght=650,CNTR=drop` or `wght=300:500:700` axis requests.
pub fn parse_axis_limits(input: &str) -> Result<Vec<AxisLimits>, SubsetError> {
    let invalid = || SubsetError::InvalidAxis(input.into());
    if input.trim().is_empty() {
        return Ok(vec![]);
    }
    input
        .split(',')
        .map(|part| {
            let (tag, value) = part.trim().split_once('=').ok_or_else(invalid)?;
            let tag = Tag::new(tag.as_bytes().try_into().map_err(|_| invalid())?);
            if value == "drop" {
                return Ok(AxisLimits::Drop { tag });
            }
            let values = value
                .split(':')
                .map(|s| s.parse::<f32>().map_err(|_| invalid()))
                .collect::<Result<Vec<_>, _>>()?;
            if values.iter().any(|v| !v.is_finite()) {
                return Err(invalid());
            }
            Ok(match values.as_slice() {
                [value] => AxisLimits::Pin { tag, value: *value },
                [min, max] => AxisLimits::Range {
                    tag,
                    min: *min,
                    default: None,
                    max: *max,
                },
                [min, default, max] => AxisLimits::Range {
                    tag,
                    min: *min,
                    default: Some(*default),
                    max: *max,
                },
                _ => return Err(invalid()),
            })
        })
        .collect()
}

pub(crate) struct AxisPlan {
    pub coords: Vec<F2Dot14>,
    pub pinned: Vec<bool>,
    pub values: Vec<(Tag, f32)>,
    pub normalized: Vec<Triple>,
    pub distances: Vec<(f64, f64)>,
    user: Vec<Triple>,
}
impl AxisPlan {
    pub fn new(font: &FontRef, limits: &[AxisLimits]) -> Result<Self, SubsetError> {
        let axes = font.axes();
        let mut pinned = vec![false; axes.len()];
        let mut settings = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut user: Vec<_> = axes
            .iter()
            .map(|a| {
                Triple(
                    a.min_value() as f64,
                    a.default_value() as f64,
                    a.max_value() as f64,
                )
            })
            .collect();
        for &limit in limits {
            let tag = limit.tag();
            let axis = axes
                .get_by_tag(tag)
                .ok_or_else(|| SubsetError::InvalidAxis(format!("unknown axis {tag}")))?;
            if !seen.insert(tag) {
                return Err(SubsetError::InvalidAxis(format!("duplicate axis {tag}")));
            }
            let value = match limit {
                AxisLimits::Drop { .. } => axis.default_value(),
                AxisLimits::Pin { value, .. } => {
                    pinned[axis.index()] = true;
                    value
                }
                AxisLimits::Range {
                    min, default, max, ..
                } => {
                    let default =
                        default.unwrap_or(axis.default_value().clamp(min.min(max), max.max(min)));
                    if !min.is_finite()
                        || !max.is_finite()
                        || !default.is_finite()
                        || min > default
                        || default > max
                        || min < axis.min_value()
                        || max > axis.max_value()
                    {
                        return Err(SubsetError::InvalidAxis(format!("invalid range for {tag}")));
                    }
                    pinned[axis.index()] = min == max;
                    default
                }
            };
            if matches!(limit, AxisLimits::Drop { .. }) {
                pinned[axis.index()] = true;
            }
            if !value.is_finite() {
                return Err(SubsetError::InvalidAxis(format!("invalid value for {tag}")));
            }
            settings.push((tag, value));
            user[axis.index()] = match limit {
                AxisLimits::Range { min, max, .. } => Triple(min as f64, value as f64, max as f64),
                _ => {
                    let value = value.clamp(axis.min_value(), axis.max_value()) as f64;
                    Triple(value, value, value)
                }
            };
        }
        if font.fvar().is_err() {
            return Err(SubsetError::InvalidAxis(
                "font has no variation axes".into(),
            ));
        }
        let mut coords = axes.location(settings.iter().copied()).coords().to_vec();
        let avar = font.avar().ok();
        if !pinned.iter().all(|p| *p)
            && avar
                .as_ref()
                .is_some_and(|a| a.version().major >= 2 && a.var_store().is_some())
        {
            return Err(SubsetError::InvalidAxis(
                "partial instancing of avar version 2 is not supported".into(),
            ));
        }
        let maps = avar
            .as_ref()
            .map(|a| a.axis_segment_maps().iter().collect::<Result<Vec<_>, _>>())
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
        let mut normalized = Vec::new();
        let mut distances = Vec::new();
        for axis in axes.iter() {
            let i = axis.index();
            let u = user[i];
            let map = |v: f64| {
                let n = F2Dot14::from_f64(
                    axis_to_normalized(
                        v,
                        axis.min_value() as f64,
                        axis.default_value() as f64,
                        axis.max_value() as f64,
                    )
                    .clamp(-1., 1.),
                );
                maps.as_ref()
                    .and_then(|m| m.get(i))
                    .map_or(n.to_f64(), |m| {
                        F2Dot14::from_f64(map_float(m, n.to_f64(), false)).to_f64()
                    })
            };
            coords[i] = F2Dot14::from_f64(map(u.1));
            normalized.push(Triple(map(u.0), coords[i].to_f64(), map(u.2)));
            distances.push((
                (axis.default_value() - axis.min_value()) as f64,
                (axis.max_value() - axis.default_value()) as f64,
            ));
        }
        if let Some(avar) = avar.as_ref() {
            if let Some(store) = avar.var_store() {
                let store = store.map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
                let index_map = avar
                    .axis_index_map()
                    .transpose()
                    .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
                let input = coords.clone();
                for (i, coord) in coords.iter_mut().enumerate() {
                    let index = if let Some(map) = &index_map {
                        map.get(i as u32)
                            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?
                    } else {
                        write_fonts::read::tables::variations::DeltaSetIndex {
                            outer: 0,
                            inner: i as u16,
                        }
                    };
                    let delta = store
                        .compute_delta(index, &input)
                        .map_or(0, |d| d.to_f64().round() as i32);
                    *coord = F2Dot14::from_bits(
                        (coord.to_bits() as i32 + delta.clamp(-32768, 32768)).clamp(-16384, 16384)
                            as i16,
                    );
                }
            }
        }
        Ok(Self {
            coords,
            pinned,
            values: settings,
            normalized,
            distances,
            user,
        })
    }
    pub fn all_pinned(&self) -> bool {
        self.pinned.iter().all(|&p| p)
    }
    pub fn new_index(&self, old: usize) -> Option<usize> {
        if *self.pinned.get(old)? {
            None
        } else {
            Some(self.pinned[..old].iter().filter(|&&p| !p).count())
        }
    }
    pub fn update_tables(
        &self,
        font: &FontRef,
        tables: &mut std::collections::BTreeMap<Tag, Vec<u8>>,
    ) -> Result<(), SubsetError> {
        use write_fonts::{
            from_obj::ToOwnedTable,
            tables::{
                avar::{Avar, AxisValueMap, SegmentMaps},
                fvar::Fvar,
            },
            types::Fixed,
        };
        let err = |tag| SubsetError::SubsetTableError(Tag::new(tag));
        let original = font.fvar().map_err(|_| err(b"fvar"))?;
        let mut fvar: Fvar = original.to_owned_table();
        let old_axes = fvar.axis_instance_arrays.axes.clone();
        fvar.axis_instance_arrays.axes = old_axes
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.pinned[*i])
            .map(|(i, a)| {
                let mut a = a.clone();
                a.min_value = Fixed::from_f64(self.user[i].0);
                a.default_value = Fixed::from_f64(self.user[i].1);
                a.max_value = Fixed::from_f64(self.user[i].2);
                a
            })
            .collect();
        fvar.axis_instance_arrays.instances.retain(|inst| {
            inst.coordinates.len() == self.user.len()
                && inst.coordinates.iter().enumerate().all(|(i, v)| {
                    let v = v.to_f64();
                    v >= self.user[i].0 && v <= self.user[i].2
                })
        });
        for inst in &mut fvar.axis_instance_arrays.instances {
            inst.coordinates = inst
                .coordinates
                .iter()
                .enumerate()
                .filter(|(i, _)| !self.pinned[*i])
                .map(|(_, v)| *v)
                .collect();
        }
        tables.insert(
            Tag::new(b"fvar"),
            write_fonts::dump_table(&fvar).map_err(|_| err(b"fvar"))?,
        );
        let avar = font.avar().ok();
        let maps = avar
            .as_ref()
            .map(|a| a.axis_segment_maps().iter().collect::<Result<Vec<_>, _>>())
            .transpose()
            .map_err(|_| err(b"avar"))?;
        let mut segment_maps = Vec::new();
        for (i, axis) in old_axes
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.pinned[*i])
        {
            let u = self.user[i];
            let normalize = |v| {
                F2Dot14::from_f64(axis_to_normalized(
                    v,
                    axis.min_value.to_f64(),
                    axis.default_value.to_f64(),
                    axis.max_value.to_f64(),
                ))
                .to_f64()
            };
            let map = maps.as_ref().and_then(|m| m.get(i));
            let pre = if let Some(map) = map {
                let n = self.normalized[i];
                Triple(
                    map_float(map, n.0, true),
                    map_float(map, n.1, true),
                    map_float(map, n.2, true),
                )
            } else {
                Triple(normalize(u.0), normalize(u.1), normalize(u.2))
            };
            let mut knots = Vec::new();
            if let Some(map) = map {
                knots.extend(
                    map.axis_value_maps()
                        .iter()
                        .map(|v| v.from_coordinate().to_f64())
                        .filter(|v| *v >= pre.0 && *v <= pre.2),
                );
            }
            knots.sort_by(f64::total_cmp);
            knots.dedup();
            let mut values = std::collections::BTreeMap::new();
            for v in knots {
                let to = map.map_or(v, |m| map_float(m, v, false));
                let from = F2Dot14::from_f64(renormalize(v, pre, self.distances[i]));
                let to = F2Dot14::from_f64(
                    renormalize(to, self.normalized[i], self.distances[i]).clamp(-1., 1.),
                );
                values.insert(from, to);
            }
            for v in [-1., 0., 1.] {
                let v = F2Dot14::from_f64(v);
                values.insert(v, v);
            }
            segment_maps.push(SegmentMaps::new(
                values
                    .into_iter()
                    .map(|(from, to)| AxisValueMap::new(from, to))
                    .collect(),
            ));
        }
        tables.insert(
            Tag::new(b"avar"),
            write_fonts::dump_table(&Avar::new(segment_maps)).map_err(|_| err(b"avar"))?,
        );
        Ok(())
    }
}
fn axis_to_normalized(v: f64, min: f64, def: f64, max: f64) -> f64 {
    if v == def {
        0.
    } else if v < def {
        if min == def {
            -1.
        } else {
            (v - def) / (def - min)
        }
    } else if max == def {
        1.
    } else {
        (v - def) / (max - def)
    }
}

// HarfBuzz reconstructs the pre-avar interval from the quantized mapped
// interval. Use the same float interpolation, including duplicate-cap recovery.
fn map_float(map: &write_fonts::read::tables::avar::SegmentMaps, v: f64, inverse: bool) -> f64 {
    let v = v as f32;
    let mut pairs: Vec<_> = map
        .axis_value_maps()
        .iter()
        .map(|m| {
            let from = m.from_coordinate().to_f64() as f32;
            let to = m.to_coordinate().to_f64() as f32;
            if inverse {
                (to, from)
            } else {
                (from, to)
            }
        })
        .collect();
    if pairs.len() < 2 {
        return pairs.first().map_or(v, |(from, to)| v - from + to) as f64;
    }
    if pairs[0] == (-1., -1.) && pairs[1].0 == -1. {
        pairs.remove(0);
    }
    let n = pairs.len();
    if n >= 2 && pairs[n - 1] == (1., 1.) && pairs[n - 2].0 == 1. {
        pairs.pop();
    }
    if let Some(i) = pairs.iter().position(|p| p.0 == v) {
        let mut j = i;
        while j + 1 < pairs.len() && pairs[j + 1].0 == v {
            j += 1;
        }
        let to = if i == j {
            pairs[i].1
        } else if j == i + 2 {
            pairs[i + 1].1
        } else if v < 0. {
            pairs[j].1
        } else if v > 0. || pairs[i].1.abs() < pairs[j].1.abs() {
            pairs[i].1
        } else {
            pairs[j].1
        };
        return to as f64;
    }
    let i = pairs.partition_point(|p| p.0 < v);
    let (from, to) = if i == 0 {
        pairs[0]
    } else if i == pairs.len() {
        pairs[i - 1]
    } else {
        let (bf, bt) = pairs[i - 1];
        let (af, at) = pairs[i];
        return (bt + (at - bt) * (v - bf) / (af - bf)) as f64;
    };
    (v - from + to) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_request_syntax() {
        assert_eq!(
            parse_axis_limits("wght=650,CNTR=drop").unwrap(),
            vec![
                AxisLimits::Pin {
                    tag: Tag::new(b"wght"),
                    value: 650.
                },
                AxisLimits::Drop {
                    tag: Tag::new(b"CNTR")
                }
            ]
        );
        assert!(matches!(
            parse_axis_limits("wght=300:500:700").unwrap()[0],
            AxisLimits::Range {
                default: Some(500.),
                ..
            }
        ));
        for invalid in ["wght=NaN", "abc=1", "wght=1:2:3:4", "wght", "wght="] {
            assert!(parse_axis_limits(invalid).is_err());
        }
    }
}
