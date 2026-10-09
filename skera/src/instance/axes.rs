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
        }
        if font.fvar().is_err() {
            return Err(SubsetError::InvalidAxis(
                "font has no variation axes".into(),
            ));
        }
        let coords = axes.location(settings.iter().copied()).coords().to_vec();
        Ok(Self {
            coords,
            pinned,
            values: settings,
        })
    }
    pub fn all_pinned(&self) -> bool {
        self.pinned.iter().all(|&p| p)
    }
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
