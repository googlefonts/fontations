//! Instance one borrowed paint node at a time. Child graphs stay in the serializer.
use super::*;
use crate::{
    offset::{SerializeCopy, SerializeSubset},
    serialize::{OffsetWhence, SerializeErrorFlags, Serializer},
    FastHashMap, Plan,
};
use read_fonts::{types::Offset24, FontRef, TableProvider};
use std::collections::BTreeSet;

#[derive(Default)]
pub(crate) struct NativeColorPlan {
    deltas: FastHashMap<u32, f32>,
    constants: BTreeSet<u32>,
    pub store: Option<Vec<u8>>,
}
impl NativeColorPlan {
    pub fn delta(&self, base: u32, offset: u32) -> Option<f32> {
        if base == u32::MAX {
            Some(0.)
        } else {
            base.checked_add(offset)
                .and_then(|i| self.deltas.get(&i))
                .copied()
        }
    }
    pub fn constant(&self, base: u32, offset: u32) -> bool {
        base == u32::MAX
            || base
                .checked_add(offset)
                .is_none_or(|i| self.constants.contains(&i))
    }
    pub fn new(font: &FontRef, axes: &AxisPlan, plan: &mut Plan) -> Result<Self, SubsetError> {
        let mut result = Self::default();
        let Ok(colr) = font.colr() else {
            return Ok(result);
        };
        let store = colr
            .item_variation_store()
            .transpose()
            .map_err(|_| error())?;
        let map = colr.var_index_map().transpose().map_err(|_| error())?;
        let map = map.filter(|m| match m {
            DeltaSetIndexMap::Format0(m) => m.map_count() != 0,
            DeltaSetIndexMap::Format1(m) => m.map_count() != 0,
        });
        let scalars = store
            .as_ref()
            .and_then(|s| VariationScalars::new(s, &axes.coords));
        let transform = store
            .as_ref()
            .map(|s| {
                let mut p = StorePlan::new(s, axes)?;
                p.preserve_original_region_order(s, axes)?;
                Ok::<_, SubsetError>(p)
            })
            .transpose()?;
        let mut keys = plan
            .colr_varidx_delta_map
            .keys()
            .copied()
            .collect::<Vec<_>>();
        keys.sort_unstable();
        let mut selected = BTreeSet::new();
        let mut source_indices = FastHashMap::default();
        for &key in &keys {
            let index = map
                .as_ref()
                .map(|m| m.get(key).map_err(|_| error()))
                .transpose()?
                .unwrap_or(DeltaSetIndex {
                    outer: (key >> 16) as u16,
                    inner: key as u16,
                });
            let old = (u32::from(index.outer) << 16) | u32::from(index.inner);
            source_indices.insert(key, old);
            if old == u32::MAX {
                result.constants.insert(key);
                result.deltas.insert(key, 0.);
                continue;
            }
            let delta = if axes.coords.iter().all(|v| *v == F2Dot14::ZERO) {
                0.
            } else {
                store
                    .as_ref()
                    .zip(scalars.as_ref())
                    .and_then(|(s, g)| g.delta(s, index))
                    .ok_or_else(error)?
            };
            result.deltas.insert(key, delta);
            let constant = if axes.all_pinned() {
                true
            } else if let Some((store, transform)) = store.as_ref().zip(transform.as_ref()) {
                let data = store
                    .item_variation_data()
                    .get(usize::from(index.outer))
                    .ok_or_else(error)?
                    .map_err(|_| error())?;
                let deltas = data
                    .delta_set(index.inner)
                    .map(f64::from)
                    .collect::<Vec<_>>();
                transform.transforms[usize::from(index.outer)]
                    .residual(&deltas)?
                    .iter()
                    .all(|d| (d + 0.5).floor() == 0.)
            } else {
                true
            };
            if constant {
                result.constants.insert(key);
            } else {
                selected.insert(old);
            }
        }
        if axes.all_pinned() {
            plan.colr_new_deltaset_idx_varidx_map.clear();
            return Ok(result);
        }
        if let Some((store, transform)) = store.as_ref().zip(transform.as_ref()) {
            // Direct indices require their rows, including constants, to remain
            // consecutive. Explicit maps permit pruning and optimization.
            if map.is_none() {
                selected.extend(source_indices.values().copied().filter(|i| *i != u32::MAX));
            }
            let (rebuilt, mapping) = if map.is_none() && plan.instance_only {
                let rebuilt = transform.rebuild(store)?;
                let identity = store
                    .item_variation_data()
                    .iter()
                    .enumerate()
                    .filter_map(|(outer, data)| Some((outer, data?.ok()?)))
                    .flat_map(|(outer, data)| {
                        (0..data.item_count()).map(move |inner| {
                            let index = ((outer as u32) << 16) | u32::from(inner);
                            (index, index)
                        })
                    })
                    .collect();
                (rebuilt, identity)
            } else {
                transform.rebuild_selected(store, &selected.into_iter().collect::<Vec<_>>())?
            };
            let (rebuilt, optimized) = if map.is_some() {
                optimize::optimize(rebuilt)?
            } else {
                let identity = mapping.values().copied().map(|i| (i, i)).collect();
                (rebuilt, identity)
            };
            plan.colr_new_deltaset_idx_varidx_map.clear();
            for (new, &key) in keys.iter().enumerate() {
                let old = source_indices[&key];
                let index = mapping
                    .get(&old)
                    .and_then(|i| optimized.get(i))
                    .copied()
                    .unwrap_or(u32::MAX);
                if map.is_some() {
                    plan.colr_varidx_delta_map.insert(key, (new as u32, 0));
                    plan.colr_new_deltaset_idx_varidx_map
                        .insert(new as u32, index);
                } else {
                    plan.colr_varidx_delta_map.insert(key, (index, 0));
                }
            }
            if !rebuilt.item_variation_data.is_empty() {
                result.store = Some(crate::instance::serialize::item_store(&rebuilt)?);
            }
        }
        Ok(result)
    }
}

pub(crate) fn serialize_paint(
    source: &read_fonts::tables::colr::Paint,
    plan: &Plan,
    s: &mut Serializer,
) -> Result<bool, SerializeErrorFlags> {
    use read_fonts::tables::colr::Paint as R;
    let Some(instance) = plan.instance.as_ref() else {
        return Ok(false);
    };
    let mut child = None;
    let mut paint = match source {
        R::VarSolid(v) => Paint::VarSolid(PaintVarSolid {
            palette_index: v.palette_index(),
            alpha: v.alpha(),
            var_index_base: v.var_index_base(),
        }),
        R::VarLinearGradient(v) => Paint::VarLinearGradient(PaintVarLinearGradient {
            color_line: own_line(
                v.color_line()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            )
            .into(),
            x0: v.x0(),
            y0: v.y0(),
            x1: v.x1(),
            y1: v.y1(),
            x2: v.x2(),
            y2: v.y2(),
            var_index_base: v.var_index_base(),
        }),
        R::VarRadialGradient(v) => Paint::VarRadialGradient(PaintVarRadialGradient {
            color_line: own_line(
                v.color_line()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            )
            .into(),
            x0: v.x0(),
            y0: v.y0(),
            radius0: v.radius0(),
            x1: v.x1(),
            y1: v.y1(),
            radius1: v.radius1(),
            var_index_base: v.var_index_base(),
        }),
        R::VarSweepGradient(v) => Paint::VarSweepGradient(PaintVarSweepGradient {
            color_line: own_line(
                v.color_line()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            )
            .into(),
            center_x: v.center_x(),
            center_y: v.center_y(),
            start_angle: v.start_angle(),
            end_angle: v.end_angle(),
            var_index_base: v.var_index_base(),
        }),
        R::VarTransform(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarTransform(PaintVarTransform {
                paint: Paint::default().into(),
                transform: own_transform(
                    v.transform()
                        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
                )
                .into(),
            })
        }
        R::VarTranslate(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarTranslate(PaintVarTranslate {
                paint: Paint::default().into(),
                dx: v.dx(),
                dy: v.dy(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarScale(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarScale(PaintVarScale {
                paint: Paint::default().into(),
                scale_x: v.scale_x(),
                scale_y: v.scale_y(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarScaleAroundCenter(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarScaleAroundCenter(PaintVarScaleAroundCenter {
                paint: Paint::default().into(),
                scale_x: v.scale_x(),
                scale_y: v.scale_y(),
                center_x: v.center_x(),
                center_y: v.center_y(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarScaleUniform(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarScaleUniform(PaintVarScaleUniform {
                paint: Paint::default().into(),
                scale: v.scale(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarScaleUniformAroundCenter(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarScaleUniformAroundCenter(PaintVarScaleUniformAroundCenter {
                paint: Paint::default().into(),
                scale: v.scale(),
                center_x: v.center_x(),
                center_y: v.center_y(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarRotate(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarRotate(PaintVarRotate {
                paint: Paint::default().into(),
                angle: v.angle(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarRotateAroundCenter(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarRotateAroundCenter(PaintVarRotateAroundCenter {
                paint: Paint::default().into(),
                angle: v.angle(),
                center_x: v.center_x(),
                center_y: v.center_y(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarSkew(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarSkew(PaintVarSkew {
                paint: Paint::default().into(),
                x_skew_angle: v.x_skew_angle(),
                y_skew_angle: v.y_skew_angle(),
                var_index_base: v.var_index_base(),
            })
        }
        R::VarSkewAroundCenter(v) => {
            child = Some(
                v.paint()
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
            );
            Paint::VarSkewAroundCenter(PaintVarSkewAroundCenter {
                paint: Paint::default().into(),
                x_skew_angle: v.x_skew_angle(),
                y_skew_angle: v.y_skew_angle(),
                center_x: v.center_x(),
                center_y: v.center_y(),
                var_index_base: v.var_index_base(),
            })
        }
        _ => return Ok(false),
    };
    let c = Context {
        axes: &instance.consumer_axes,
        store: None,
        map: None,
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        native: Some(&instance.color),
    };
    c.paint(&mut paint, 0)
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
    serialize_local(&paint, child.as_ref(), plan, s)?;
    Ok(true)
}
fn palette(index: u16, plan: &Plan) -> Result<u16, SerializeErrorFlags> {
    if index == u16::MAX {
        Ok(index)
    } else {
        plan.colr_palettes
            .get(&index)
            .copied()
            .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_OTHER)
    }
}
fn base(index: u32, plan: &Plan) -> u32 {
    if index == u32::MAX {
        index
    } else {
        plan.colr_varidx_delta_map
            .get(&index)
            .map_or(u32::MAX, |v| v.0)
    }
}
fn child_offset(
    child: Option<&read_fonts::tables::colr::Paint>,
    plan: &Plan,
    s: &mut Serializer,
) -> Result<(), SerializeErrorFlags> {
    let pos = s.allocate_size(3, true)?;
    Offset24::serialize_subset(
        child.ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?,
        s,
        plan,
        (),
        pos,
    )
}
fn line(
    line: &ColorLine,
    plan: &Plan,
    s: &mut Serializer,
    pos: usize,
) -> Result<(), SerializeErrorFlags> {
    s.push()?;
    s.embed(line.extend)?;
    s.embed(line.color_stops.len() as u16)?;
    for stop in &line.color_stops {
        s.embed(stop.stop_offset)?;
        s.embed(palette(stop.palette_index, plan)?)?;
        s.embed(stop.alpha)?;
    }
    let obj = s.pop_pack(true).ok_or(s.error())?;
    s.add_link(pos..pos + 3, obj, OffsetWhence::Head, 0, false)
}
fn var_line(
    line: &VarColorLine,
    plan: &Plan,
    s: &mut Serializer,
    pos: usize,
) -> Result<(), SerializeErrorFlags> {
    s.push()?;
    s.embed(line.extend)?;
    s.embed(line.color_stops.len() as u16)?;
    for stop in &line.color_stops {
        s.embed(stop.stop_offset)?;
        s.embed(palette(stop.palette_index, plan)?)?;
        s.embed(stop.alpha)?;
        s.embed(base(stop.var_index_base, plan))?;
    }
    let obj = s.pop_pack(true).ok_or(s.error())?;
    s.add_link(pos..pos + 3, obj, OffsetWhence::Head, 0, false)
}
fn serialize_local(
    paint: &Paint,
    child: Option<&read_fonts::tables::colr::Paint>,
    plan: &Plan,
    s: &mut Serializer,
) -> Result<(), SerializeErrorFlags> {
    match paint {
        Paint::Solid(v) => {
            s.embed(2u8)?;
            s.embed(palette(v.palette_index, plan)?)?;
            s.embed(v.alpha)?;
        }
        Paint::VarSolid(v) => {
            s.embed(3u8)?;
            s.embed(palette(v.palette_index, plan)?)?;
            s.embed(v.alpha)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::LinearGradient(v) => {
            s.embed(4u8)?;
            let pos = s.allocate_size(3, true)?;
            line(&v.color_line, plan, s, pos)?;
            s.embed(v.x0)?;
            s.embed(v.y0)?;
            s.embed(v.x1)?;
            s.embed(v.y1)?;
            s.embed(v.x2)?;
            s.embed(v.y2)?;
        }
        Paint::VarLinearGradient(v) => {
            s.embed(5u8)?;
            let pos = s.allocate_size(3, true)?;
            var_line(&v.color_line, plan, s, pos)?;
            s.embed(v.x0)?;
            s.embed(v.y0)?;
            s.embed(v.x1)?;
            s.embed(v.y1)?;
            s.embed(v.x2)?;
            s.embed(v.y2)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::RadialGradient(v) => {
            s.embed(6u8)?;
            let pos = s.allocate_size(3, true)?;
            line(&v.color_line, plan, s, pos)?;
            s.embed(v.x0)?;
            s.embed(v.y0)?;
            s.embed(v.radius0)?;
            s.embed(v.x1)?;
            s.embed(v.y1)?;
            s.embed(v.radius1)?;
        }
        Paint::VarRadialGradient(v) => {
            s.embed(7u8)?;
            let pos = s.allocate_size(3, true)?;
            var_line(&v.color_line, plan, s, pos)?;
            s.embed(v.x0)?;
            s.embed(v.y0)?;
            s.embed(v.radius0)?;
            s.embed(v.x1)?;
            s.embed(v.y1)?;
            s.embed(v.radius1)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::SweepGradient(v) => {
            s.embed(8u8)?;
            let pos = s.allocate_size(3, true)?;
            line(&v.color_line, plan, s, pos)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
            s.embed(v.start_angle)?;
            s.embed(v.end_angle)?;
        }
        Paint::VarSweepGradient(v) => {
            s.embed(9u8)?;
            let pos = s.allocate_size(3, true)?;
            var_line(&v.color_line, plan, s, pos)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
            s.embed(v.start_angle)?;
            s.embed(v.end_angle)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::Transform(v) => {
            s.embed(12u8)?;
            child_offset(child, plan, s)?;
            let pos = s.allocate_size(3, true)?;
            let mut bytes = Vec::new();
            for value in [
                v.transform.xx,
                v.transform.yx,
                v.transform.xy,
                v.transform.yy,
                v.transform.dx,
                v.transform.dy,
            ] {
                bytes.extend(value.to_bits().to_be_bytes());
            }
            Offset24::serialize_copy_from_bytes(&bytes, s, pos)?;
        }
        Paint::VarTransform(v) => {
            s.embed(13u8)?;
            child_offset(child, plan, s)?;
            let pos = s.allocate_size(3, true)?;
            let mut bytes = Vec::new();
            for value in [
                v.transform.xx,
                v.transform.yx,
                v.transform.xy,
                v.transform.yy,
                v.transform.dx,
                v.transform.dy,
            ] {
                bytes.extend(value.to_bits().to_be_bytes());
            }
            bytes.extend(base(v.transform.var_index_base, plan).to_be_bytes());
            Offset24::serialize_copy_from_bytes(&bytes, s, pos)?;
        }
        Paint::Translate(v) => {
            s.embed(14u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.dx)?;
            s.embed(v.dy)?;
        }
        Paint::VarTranslate(v) => {
            s.embed(15u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.dx)?;
            s.embed(v.dy)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::Scale(v) => {
            s.embed(16u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale_x)?;
            s.embed(v.scale_y)?;
        }
        Paint::VarScale(v) => {
            s.embed(17u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale_x)?;
            s.embed(v.scale_y)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::ScaleAroundCenter(v) => {
            s.embed(18u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale_x)?;
            s.embed(v.scale_y)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
        }
        Paint::VarScaleAroundCenter(v) => {
            s.embed(19u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale_x)?;
            s.embed(v.scale_y)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::ScaleUniform(v) => {
            s.embed(20u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale)?;
        }
        Paint::VarScaleUniform(v) => {
            s.embed(21u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::ScaleUniformAroundCenter(v) => {
            s.embed(22u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
        }
        Paint::VarScaleUniformAroundCenter(v) => {
            s.embed(23u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.scale)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::Rotate(v) => {
            s.embed(24u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.angle)?;
        }
        Paint::VarRotate(v) => {
            s.embed(25u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.angle)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::RotateAroundCenter(v) => {
            s.embed(26u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.angle)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
        }
        Paint::VarRotateAroundCenter(v) => {
            s.embed(27u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.angle)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::Skew(v) => {
            s.embed(28u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.x_skew_angle)?;
            s.embed(v.y_skew_angle)?;
        }
        Paint::VarSkew(v) => {
            s.embed(29u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.x_skew_angle)?;
            s.embed(v.y_skew_angle)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        Paint::SkewAroundCenter(v) => {
            s.embed(30u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.x_skew_angle)?;
            s.embed(v.y_skew_angle)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
        }
        Paint::VarSkewAroundCenter(v) => {
            s.embed(31u8)?;
            child_offset(child, plan, s)?;
            s.embed(v.x_skew_angle)?;
            s.embed(v.y_skew_angle)?;
            s.embed(v.center_x)?;
            s.embed(v.center_y)?;
            s.embed(base(v.var_index_base, plan))?;
        }
        _ => return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER)),
    }
    Ok(())
}
pub(crate) fn clip(
    v: &read_fonts::tables::colr::ClipBoxFormat2,
    plan: &Plan,
    s: &mut Serializer,
) -> Result<(), SerializeErrorFlags> {
    let instance = plan.instance.as_ref().unwrap();
    let c = Context {
        axes: &instance.consumer_axes,
        store: None,
        map: None,
        scalars: OnceCell::new(),
        store_plan: OnceCell::new(),
        native: Some(&instance.color),
    };
    s.embed(if instance.consumer_axes.all_pinned() {
        1u8
    } else {
        2u8
    })?;
    for (i, mut value) in [v.x_min(), v.y_min(), v.x_max(), v.y_max()]
        .into_iter()
        .enumerate()
    {
        c.clip_edge(v.var_index_base(), i as u32, &mut value)
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        s.embed(value)?;
    }
    if !instance.consumer_axes.all_pinned() {
        s.embed(base(v.var_index_base(), plan))?;
    }
    Ok(())
}

fn own_line(v: read_fonts::tables::colr::VarColorLine) -> VarColorLine {
    v.to_owned_table()
}
fn own_transform(v: read_fonts::tables::colr::VarAffine2x3) -> VarAffine2x3 {
    v.to_owned_table()
}
