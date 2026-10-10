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
    /// Pin an axis at the specified user coordinate.
    Pin { tag: Tag, value: f32 },
    /// Pin an axis at its original default.
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

#[derive(Clone)]
pub(crate) struct AxisPlan {
    pub coords: Vec<F2Dot14>,
    pub metric_coords: Vec<F2Dot14>,
    pub pinned: Vec<bool>,
    pub values: Vec<(Tag, f32)>,
    pub normalized: Vec<Triple>,
    pub distances: Vec<(f64, f64)>,
    pub(super) user: Vec<Triple>,
    pub(super) user_pinned: Vec<bool>,
    // avar2 retains the old final-coordinate space in all other tables.
    pub(super) coupled: bool,
    pub(super) reachable: Vec<Option<(i16, i16)>>,
}
impl AxisPlan {
    pub fn new(font: &FontRef, limits: &[AxisLimits]) -> Result<Self, SubsetError> {
        let axes = font.axes();
        if axes
            .iter()
            .any(|a| a.min_value() > a.default_value() || a.default_value() > a.max_value())
        {
            return Err(SubsetError::SubsetTableError(Tag::new(b"fvar")));
        }
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
        for axis in axes.iter() {
            pinned[axis.index()] = axis.min_value() == axis.max_value();
        }
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
                    if !min.is_finite() || !max.is_finite() || min > max {
                        return Err(SubsetError::InvalidAxis(format!("invalid range for {tag}")));
                    }
                    let default = default.unwrap_or(axis.default_value().clamp(min, max));
                    if !default.is_finite()
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
            let value = value.clamp(axis.min_value(), axis.max_value());
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
        let coupled = !pinned.iter().all(|p| *p)
            && avar
                .as_ref()
                .is_some_and(|a| a.version().major >= 2 && a.var_store().is_some());
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
                let n = normalized_coord(
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
                        normalized_coord(map_float(m, n.to_f64(), false)).to_f64()
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
                let scalars = super::scalars::VariationScalars::new(&store, &input)
                    .ok_or(SubsetError::SubsetTableError(Tag::new(b"avar")))?;
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
                    let delta = scalars
                        .delta(&store, index)
                        .map_or(0, |d| (d.clamp(-32768., 32768.) + 0.5).floor() as i32);
                    *coord = F2Dot14::from_bits(
                        (coord.to_bits() as i32 + delta.clamp(-32768, 32768)).clamp(-16384, 16384)
                            as i16,
                    );
                }
            }
        }
        let user_pinned = pinned.clone();
        // HarfBuzz uses its font's 16.16 normalization path for CFF bounds and
        // advances, while outline/store instancing uses the 2.14 plan above.
        let mut metric_coords: Vec<i32> = axes
            .iter()
            .map(|axis| {
                let n = axis_to_normalized(
                    user[axis.index()].1,
                    axis.min_value() as f64,
                    axis.default_value() as f64,
                    axis.max_value() as f64,
                ) as f32;
                (n * 65536. + 0.5).floor() as i32
            })
            .collect();
        if let Some(maps) = &maps {
            for (coord, map) in metric_coords.iter_mut().zip(maps) {
                let mapped = map_float(map, *coord as f64 / 65536., false) as f32;
                *coord = (mapped * 65536. + 0.5).floor() as i32;
            }
        }
        if let Some(avar) = &avar {
            if let Some(store) = avar.var_store() {
                let store = store.map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
                let intermediate: Vec<_> = metric_coords
                    .iter()
                    .map(|c| F2Dot14::from_bits(((c + 2) >> 2) as i16))
                    .collect();
                let scalars = super::scalars::VariationScalars::new(&store, &intermediate)
                    .ok_or(SubsetError::SubsetTableError(Tag::new(b"avar")))?;
                let map = avar
                    .axis_index_map()
                    .transpose()
                    .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
                for (i, coord) in metric_coords.iter_mut().enumerate() {
                    let index = if let Some(map) = &map {
                        map.get(i as u32)
                            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?
                    } else {
                        write_fonts::read::tables::variations::DeltaSetIndex {
                            outer: 0,
                            inner: i as u16,
                        }
                    };
                    let delta =
                        (scalars.delta(&store, index).unwrap_or(0.) * 4.).clamp(-131072., 131072.);
                    *coord = (*coord + (delta + 0.5).floor() as i32).clamp(-65536, 65536);
                }
            }
        }
        let metric_coords = metric_coords
            .into_iter()
            .map(|c| F2Dot14::from_bits(((c + 2) >> 2) as i16))
            .collect::<Vec<_>>();
        if coupled {
            pinned.fill(false);
        }
        let mut plan = Self {
            coords,
            metric_coords,
            pinned,
            values: settings,
            normalized,
            distances,
            user,
            user_pinned,
            coupled,
            reachable: vec![None; axes.len()],
        };
        if coupled {
            let reachability = super::avar2::reachable_ranges(font, &plan)?;
            plan.reachable = reachability.ranges;
            for (i, pin) in reachability.pins.into_iter().enumerate() {
                if let Some(coord) = pin {
                    plan.pinned[i] = true;
                    plan.coords[i] = coord;
                    plan.metric_coords[i] = coord;
                }
            }
        }
        Ok(plan)
    }
    // Other variation tables see only constant final-coordinate pins. The
    // remaining user restrictions are carried by the avar2 transform.
    pub(super) fn final_space(&self, font: &FontRef) -> Self {
        let mut plan = self.clone();
        for (i, &pinned) in self.pinned.iter().enumerate() {
            plan.normalized[i] = if pinned {
                let v = self.coords[i].to_f64();
                Triple(v, v, v)
            } else {
                plan.coords[i] = F2Dot14::ZERO;
                plan.metric_coords[i] = F2Dot14::ZERO;
                Triple(-1., 0., 1.)
            };
        }
        plan.values.retain(|(tag, _)| {
            font.axes()
                .get_by_tag(*tag)
                .is_some_and(|a| self.pinned[a.index()])
        });
        plan
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
                // HarfBuzz copies free axis records verbatim. Converting an
                // untouched 16.16 value through float can lose its low bit.
                if self.values.iter().any(|(tag, _)| *tag == a.axis_tag) {
                    a.min_value = Fixed::from_f64(self.user[i].0);
                    a.default_value = Fixed::from_f64(self.user[i].1);
                    a.max_value = Fixed::from_f64(self.user[i].2);
                }
                if self.coupled && self.user_pinned[i] {
                    a.flags |= 1; // Hidden, but its final coordinate can still vary.
                }
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
            // SegmentMaps::subset copies maps for axes absent from the
            // requested location. Inverting and rebuilding those maps can
            // alter flat segments even though the axis was left unchanged.
            if !self.values.iter().any(|(tag, _)| *tag == axis.axis_tag) {
                segment_maps.push(maps.as_ref().and_then(|m| m.get(i)).map_or_else(
                    || {
                        SegmentMaps::new(
                            [-1., 0., 1.]
                                .into_iter()
                                .map(|v| {
                                    AxisValueMap::new(F2Dot14::from_f64(v), F2Dot14::from_f64(v))
                                })
                                .collect(),
                        )
                    },
                    |m| {
                        SegmentMaps::new(
                            m.axis_value_maps()
                                .iter()
                                .map(|v| AxisValueMap::new(v.from_coordinate(), v.to_coordinate()))
                                .collect(),
                        )
                    },
                ));
                continue;
            }
            if self.coupled && self.user_pinned[i] {
                segment_maps.push(SegmentMaps::new(
                    [-1., 0., 1.]
                        .into_iter()
                        .map(|v| AxisValueMap::new(F2Dot14::from_f64(v), F2Dot14::from_f64(v)))
                        .collect(),
                ));
                continue;
            }
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
        let avar = if self.coupled {
            super::avar2::instance(font, self, segment_maps)?
        } else {
            Avar::new(segment_maps)
        };
        tables.insert(
            Tag::new(b"avar"),
            write_fonts::dump_table(&avar).map_err(|_| err(b"avar"))?,
        );
        Ok(())
    }
}
pub(super) fn axis_to_normalized(v: f64, min: f64, def: f64, max: f64) -> f64 {
    // HarfBuzz normalizes the user value in float before rounding to 2.14.
    // Double intermediates can land on the other side of a half-unit tie.
    let (v, min, def, max) = (v as f32, min as f32, def as f32, max as f32);
    let normalized = if v == def {
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
    };
    normalized as f64
}

pub(super) fn normalized_coord(value: f64) -> F2Dot14 {
    F2Dot14::from_bits(((value * 16384. + 0.5).floor().clamp(-32768., 32767.)) as i16)
}

// HarfBuzz reconstructs the pre-avar interval from the quantized mapped
// interval. Use the same float interpolation, including duplicate-cap recovery.
pub(super) fn map_float(
    map: &write_fonts::read::tables::avar::SegmentMaps,
    v: f64,
    inverse: bool,
) -> f64 {
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
    fn nonfinite_api_ranges_return_errors() {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        for (min, max) in [
            (f32::NAN, f32::NAN),
            (f32::NAN, 900.),
            (f32::NEG_INFINITY, f32::INFINITY),
            (900., 100.),
        ] {
            assert!(matches!(
                crate::instance_font(
                    &font,
                    &[AxisLimits::Range {
                        tag: Tag::new(b"wght"),
                        min,
                        default: None,
                        max,
                    }]
                ),
                Err(SubsetError::InvalidAxis(_))
            ));
        }
    }

    #[test]
    fn untouched_axis_records_preserve_all_fixed_point_bits() {
        use write_fonts::{from_obj::ToOwnedTable, tables::fvar::Fvar};
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let original: Fvar = font.fvar().unwrap().to_owned_table();
        let expected = &original.axis_instance_arrays.axes[0];
        assert_ne!(
            expected.default_value.to_f64(),
            expected.default_value.to_f32() as f64
        );
        for request in ["CNTR=drop", "CNTR=25:50:75"] {
            let bytes = crate::instance_font(&font, &parse_axis_limits(request).unwrap()).unwrap();
            let instance = FontRef::new(&bytes).unwrap();
            let actual: Fvar = instance.fvar().unwrap().to_owned_table();
            assert_eq!(&actual.axis_instance_arrays.axes[0], expected, "{request}");
        }
    }

    #[test]
    fn invalid_source_axis_bounds_return_an_error() {
        use write_fonts::{from_obj::ToOwnedTable, tables::fvar::Fvar, FontBuilder};
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let mut fvar: Fvar = font.fvar().unwrap().to_owned_table();
        fvar.axis_instance_arrays.axes[0].min_value = write_fonts::types::Fixed::from_f64(1000.);
        let mut builder = FontBuilder::new();
        for r in font.table_directory().table_records() {
            builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
        }
        builder.add_table(&fvar).unwrap();
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        assert!(matches!(
            crate::instance_font(&font, &parse_axis_limits("wght=700").unwrap()),
            Err(SubsetError::SubsetTableError(tag)) if tag == Tag::new(b"fvar")
        ));
    }

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
