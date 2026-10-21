use crate::SubsetError;
use read_fonts::{
    types::{F2Dot14, Tag},
    FontRef, TableProvider,
};
use skrifa::MetadataProvider;

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
    pub metric_coords: Vec<F2Dot14>,
    pub pinned: Vec<bool>,
    pub values: Vec<(Tag, f32)>,
}
impl AxisPlan {
    pub fn new(font: &FontRef, limits: &[AxisLimits]) -> Result<Self, SubsetError> {
        let axes = font.axes();
        let mut pinned = vec![false; axes.len()];
        let mut settings = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
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
            settings.push((tag, value.clamp(axis.min_value(), axis.max_value())));
        }
        if font.fvar().is_err() {
            return Err(SubsetError::InvalidAxis(
                "font has no variation axes".into(),
            ));
        }
        // Font metrics use 16.16 normalization before conversion to 2.14.
        let metric_coords = axes.location(settings.iter().copied()).coords().to_vec();
        let mut coords = Vec::with_capacity(axes.len());
        let avar = font.avar().ok();
        if avar.as_ref().is_some_and(|a| a.var_store().is_some()) {
            // Axis coupling is introduced by the avar2 stack member.
            return Err(SubsetError::SubsetTableError(Tag::new(b"avar")));
        }
        let maps = avar
            .as_ref()
            .map(|a| a.axis_segment_maps().iter().collect::<Result<Vec<_>, _>>())
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
        for axis in axes.iter() {
            let value = settings
                .iter()
                .find(|(tag, _)| *tag == axis.tag())
                .map_or(axis.default_value(), |(_, value)| *value);
            let n = normalized_coord(axis_to_normalized(
                value as f64,
                axis.min_value() as f64,
                axis.default_value() as f64,
                axis.max_value() as f64,
            ));
            coords.push(
                maps.as_ref()
                    .and_then(|m| m.get(axis.index()))
                    .map_or(n, |m| normalized_coord(map_float(m, n.to_f64(), false))),
            );
        }
        Ok(Self {
            coords,
            metric_coords,
            pinned,
            values: settings,
        })
    }
    pub fn all_pinned(&self) -> bool {
        self.pinned.iter().all(|&p| p)
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
pub(super) fn map_float(map: &read_fonts::tables::avar::SegmentMaps, v: f64, inverse: bool) -> f64 {
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
