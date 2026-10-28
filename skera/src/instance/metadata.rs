//! fvar and avar own their output metadata; AxisPlan is read-only input.
use super::{
    axes::{axis_to_normalized, map_float, normalized_coord},
    rebase::{renormalize, Triple},
    FontContext, Instance,
};
use crate::SubsetError;
use read_fonts::types::{F2Dot14, Fixed, Tag};
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::{
        avar::{Avar, AxisValueMap, SegmentMaps},
        fvar::Fvar,
    },
};

pub(crate) struct FvarInstance {
    pub bytes: Vec<u8>,
    pub original_axes: Vec<write_fonts::tables::fvar::VariationAxisRecord>,
}
pub(super) struct AvarContext<'a, 'b> {
    pub input: &'a FontContext<'b>,
    pub original_axes: &'a [write_fonts::tables::fvar::VariationAxisRecord],
}

impl Instance<FontContext<'_>> for read_fonts::tables::fvar::Fvar<'_> {
    type Output = FvarInstance;
    fn instance(&self, context: &mut FontContext<'_>) -> Result<Self::Output, SubsetError> {
        let axes = context.axes;
        let err = |tag| SubsetError::SubsetTableError(Tag::new(tag));
        let original = self;
        let mut fvar: Fvar = original.to_owned_table();
        let old_axes = fvar.axis_instance_arrays.axes.clone();
        fvar.axis_instance_arrays.axes = old_axes
            .iter()
            .enumerate()
            .filter(|(i, _)| !axes.pinned[*i])
            .map(|(i, a)| {
                let mut a = a.clone();
                // HarfBuzz copies free axis records verbatim. Converting an
                // untouched 16.16 value through float can lose its low bit.
                if axes.values.iter().any(|(tag, _)| *tag == a.axis_tag) {
                    a.min_value = Fixed::from_f64(axes.user[i].0);
                    a.default_value = Fixed::from_f64(axes.user[i].1);
                    a.max_value = Fixed::from_f64(axes.user[i].2);
                }
                if axes.coupled && axes.user_pinned[i] {
                    a.flags |= 1; // Hidden, but its final coordinate can still vary.
                }
                a
            })
            .collect();
        fvar.axis_instance_arrays.instances.retain(|inst| {
            inst.coordinates.len() == axes.user.len()
                && inst.coordinates.iter().enumerate().all(|(i, v)| {
                    let v = v.to_f64();
                    v >= axes.user[i].0 && v <= axes.user[i].2
                })
        });
        for inst in &mut fvar.axis_instance_arrays.instances {
            inst.coordinates = inst
                .coordinates
                .iter()
                .enumerate()
                .filter(|(i, _)| !axes.pinned[*i])
                .map(|(_, v)| *v)
                .collect();
        }
        Ok(FvarInstance {
            bytes: write_fonts::dump_table(&fvar).map_err(|_| err(b"fvar"))?,
            original_axes: old_axes,
        })
    }
}
impl Instance<AvarContext<'_, '_>> for read_fonts::tables::avar::Avar<'_> {
    type Output = Vec<u8>;
    fn instance(&self, context: &mut AvarContext<'_, '_>) -> Result<Self::Output, SubsetError> {
        rebuild_avar(Some(self), context)
    }
}

/// An absent source avar requires explicit identity maps, whereas a present
/// empty map must be preserved. Do not substitute a default borrowed table.
pub(super) fn rebuild_avar(
    avar: Option<&read_fonts::tables::avar::Avar>,
    context: &AvarContext<'_, '_>,
) -> Result<Vec<u8>, SubsetError> {
    let font = context.input.font;
    let axes = context.input.axes;
    let err = |tag| SubsetError::SubsetTableError(Tag::new(tag));
    let old_axes = context.original_axes;
    let maps = avar
        .as_ref()
        .map(|a| a.axis_segment_maps().iter().collect::<Result<Vec<_>, _>>())
        .transpose()
        .map_err(|_| err(b"avar"))?;
    let mut segment_maps = Vec::new();
    for (i, axis) in old_axes
        .iter()
        .enumerate()
        .filter(|(i, _)| !axes.pinned[*i])
    {
        let u = axes.user[i];
        // SegmentMaps::subset copies maps for axes absent from the
        // requested location. Inverting and rebuilding those maps can
        // alter flat segments even though the axis was left unchanged.
        if !axes.values.iter().any(|(tag, _)| *tag == axis.axis_tag) {
            segment_maps.push(maps.as_ref().and_then(|m| m.get(i)).map_or_else(
                || {
                    SegmentMaps::new(
                        [-1., 0., 1.]
                            .into_iter()
                            .map(|v| AxisValueMap::new(F2Dot14::from_f64(v), F2Dot14::from_f64(v)))
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
        if axes.coupled && axes.user_pinned[i] {
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
            let n = axes.normalized[i];
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
        let distances = (axes.distances[i].0 as f32, axes.distances[i].1 as f32);
        for v in knots {
            let to = map.map_or(v, |m| map_float(m, v, false));
            let from =
                normalized_coord(renormalize(v as f32, pre.map(|v| v as f32), distances) as f64);
            let to = normalized_coord(
                renormalize(to as f32, axes.normalized[i].map(|v| v as f32), distances)
                    .clamp(-1., 1.) as f64,
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
    let avar = if axes.coupled {
        super::avar2::instance(font, axes, segment_maps, avar.ok_or_else(|| err(b"avar"))?)?
    } else {
        Avar::new(segment_maps)
    };
    write_fonts::dump_table(&avar).map_err(|_| err(b"avar"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use read_fonts::{FontRef, TableProvider};
    use write_fonts::FontBuilder;

    #[test]
    fn partial_instance_distinguishes_absent_avar_from_empty_maps() {
        let source = std::fs::read("test-data/fonts/colr-affine-values.ttf").unwrap();
        let font = FontRef::new(&source).unwrap();
        assert!(font.avar().is_err());
        let limits = crate::parse_axis_limits("wght=0.65").unwrap();
        for with_empty_avar in [false, true] {
            let mut builder = FontBuilder::new();
            for record in font.table_directory().table_records() {
                builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
            }
            if with_empty_avar {
                let axis_count = font.fvar().unwrap().axis_count();
                builder
                    .add_table(&Avar::new(
                        (0..axis_count)
                            .map(|_| SegmentMaps::new(Vec::new()))
                            .collect(),
                    ))
                    .unwrap();
            }
            let input = builder.build();
            let output = crate::instance_font(&FontRef::new(&input).unwrap(), &limits).unwrap();
            let output = FontRef::new(&output).unwrap();
            let avar = output.avar().unwrap();
            let maps = avar
                .axis_segment_maps()
                .iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(maps.len(), 1);
            let pairs = maps[0]
                .axis_value_maps()
                .iter()
                .map(|v| (v.from_coordinate().to_f64(), v.to_coordinate().to_f64()))
                .collect::<Vec<_>>();
            let expected = if with_empty_avar {
                Vec::new()
            } else {
                vec![(-1., -1.), (0., 0.), (1., 1.)]
            };
            assert_eq!(pairs, expected, "empty source avar: {with_empty_avar}");
        }
    }
}
