//! Fold COLRv1 values and rebase its item variation store with the outline axes.
use super::{optimize, scalars::VariationScalars, AxisPlan, StorePlan};
use crate::SubsetError;
use read_fonts::{
    tables::variations::{DeltaSetIndex, DeltaSetIndexMap, ItemVariationStore},
    types::{F2Dot14, FWord, Fixed, Tag, UfWord},
};
use std::cell::OnceCell;
use write_fonts::{from_obj::ToOwnedTable, tables::colr::*};

fn error() -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(b"COLR"))
}
struct Context<'a> {
    axes: &'a AxisPlan,
    store: Option<ItemVariationStore<'a>>,
    map: Option<DeltaSetIndexMap<'a>>,
    scalars: OnceCell<Option<VariationScalars>>,
}
impl Context<'_> {
    fn delta(&self, base: u32, offset: u32) -> Result<f32, SubsetError> {
        if base == u32::MAX {
            return Ok(0.);
        }
        let index = base.checked_add(offset).ok_or_else(error)?;
        let index = if let Some(map) = &self.map {
            map.get(index).map_err(|_| error())?
        } else {
            DeltaSetIndex {
                outer: (index >> 16) as u16,
                inner: index as u16,
            }
        };
        if index == DeltaSetIndex::NO_VARIATION_INDEX {
            return Ok(0.);
        }
        let store = self.store.as_ref().ok_or_else(error)?;
        self.scalars
            .get_or_init(|| VariationScalars::new(store, &self.axes.coords))
            .as_ref()
            .and_then(|scalars| scalars.delta(store, index))
            .ok_or_else(error)
    }
    fn field<T: ColorValue>(
        &self,
        base: u32,
        offset: u32,
        value: &mut T,
    ) -> Result<(), SubsetError> {
        // HarfBuzz leaves base values untouched at the original default and
        // for the no-variation sentinel, including low bits of Fixed values.
        if base != u32::MAX && self.axes.coords.iter().any(|&v| v != F2Dot14::ZERO) {
            *value = value.add(self.delta(base, offset)?);
        }
        Ok(())
    }
    fn alpha(&self, base: u32, offset: u32, value: &mut F2Dot14) -> Result<(), SubsetError> {
        self.field(base, offset, value)?;
        // Keep the unbounded base while retained axes can still change it.
        if self.axes.all_pinned() {
            *value = F2Dot14::from_bits(value.to_bits().clamp(0, 16384));
        }
        Ok(())
    }
    fn clip_edge(&self, base: u32, offset: u32, value: &mut FWord) -> Result<(), SubsetError> {
        if base != u32::MAX && self.axes.coords.iter().any(|&v| v != F2Dot14::ZERO) {
            let delta = f64::from(self.delta(base, offset)?);
            let delta = if offset < 2 {
                delta.floor()
            } else {
                delta.ceil()
            };
            *value = FWord::new((f64::from(value.to_i16()) + delta) as i16);
        }
        Ok(())
    }
    fn scale(&self, base: u32, offset: u32, value: F2Dot14) -> Result<f64, SubsetError> {
        let delta = if base != u32::MAX && self.axes.coords.iter().any(|&v| v != F2Dot14::ZERO) {
            self.delta(base, offset)?
        } else {
            0.
        };
        Ok(super::round_f32(value.to_bits() as f32 + delta) / 16384.)
    }
    fn angle(&self, base: u32, offset: u32, value: &mut F2Dot14) -> Result<(), SubsetError> {
        if base != u32::MAX && self.axes.coords.iter().any(|&v| v != F2Dot14::ZERO) {
            let bits = super::round_f32(value.to_bits() as f32 + self.delta(base, offset)?);
            // Rotation and skew are periodic. Reduce the rounded angle modulo
            // 720 degrees, preserving its effect when it exceeds F2Dot14.
            *value = F2Dot14::from_bits(bits.rem_euclid(65536.) as u16 as i16);
        }
        Ok(())
    }
    fn line(&self, line: &mut VarColorLine) -> Result<(), SubsetError> {
        for stop in &mut line.color_stops {
            self.field(stop.var_index_base, 0, &mut stop.stop_offset)?;
            self.alpha(stop.var_index_base, 1, &mut stop.alpha)?;
        }
        Ok(())
    }
    fn radial(&self, v: &PaintVarRadialGradient) -> Result<Option<Paint>, SubsetError> {
        if !self.axes.all_pinned() {
            if !self.indices_are_constant(v.var_index_base, 6)? {
                return Ok(None);
            }
            for stop in &v.color_line.color_stops {
                if !self.indices_are_constant(stop.var_index_base, 2)? {
                    return Ok(None);
                }
            }
        }
        let values = [
            f64::from(v.x0.to_i16()),
            f64::from(v.y0.to_i16()),
            f64::from(v.radius0.to_u16()),
            f64::from(v.x1.to_i16()),
            f64::from(v.y1.to_i16()),
            f64::from(v.radius1.to_u16()),
        ];
        let mut geometry = values;
        if self.axes.coords.iter().any(|&v| v != F2Dot14::ZERO) {
            for (i, value) in geometry.iter_mut().enumerate() {
                *value += super::round_f32(self.delta(v.var_index_base, i as u32)?);
            }
        }
        if geometry[2] >= 0. && geometry[5] >= 0. {
            return Ok(None);
        }
        let mut line = static_line(&v.color_line);
        let mut offsets = Vec::new();
        for (source, stop) in v.color_line.color_stops.iter().zip(&mut line.color_stops) {
            let delta = if self.axes.coords.iter().any(|&v| v != F2Dot14::ZERO) {
                self.delta(source.var_index_base, 0)?
            } else {
                0.
            };
            offsets.push(super::round_f32(source.stop_offset.to_bits() as f32 + delta) / 16384.);
            self.alpha(source.var_index_base, 1, &mut stop.alpha)?;
        }
        nonnegative_radial(geometry, line, offsets).map(Some)
    }
    fn indices_are_constant(&self, base: u32, count: u32) -> Result<bool, SubsetError> {
        if base == u32::MAX {
            return Ok(true);
        }
        let mut plan = None;
        for offset in 0..count {
            let index = base.checked_add(offset).ok_or_else(error)?;
            let index = if let Some(map) = &self.map {
                map.get(index).map_err(|_| error())?
            } else {
                DeltaSetIndex {
                    outer: (index >> 16) as u16,
                    inner: index as u16,
                }
            };
            if index == DeltaSetIndex::NO_VARIATION_INDEX {
                continue;
            }
            let store = self.store.as_ref().ok_or_else(error)?;
            let plan = match &mut plan {
                Some(plan) => plan,
                slot @ None => slot.insert(StorePlan::new(store, self.axes).map_err(|_| error())?),
            };
            let data = store
                .item_variation_data()
                .get(index.outer as usize)
                .ok_or_else(error)?
                .map_err(|_| error())?;
            let deltas = data
                .delta_set(index.inner)
                .map(f64::from)
                .collect::<Vec<_>>();
            if plan.transforms[index.outer as usize]
                .residual(&deltas)?
                .iter()
                .any(|&delta| (delta + 0.5).floor() != 0.)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn paint(&self, paint: &mut Paint, depth: usize) -> Result<(), SubsetError> {
        if depth > 64 {
            return Err(error());
        }
        match paint {
            Paint::ColrLayers(_) => {}
            Paint::Solid(_) => {}
            Paint::VarSolid(v) => {
                self.alpha(v.var_index_base, 0, &mut v.alpha)?;
                if self.axes.all_pinned() {
                    *paint = Paint::Solid(PaintSolid {
                        palette_index: v.palette_index,
                        alpha: v.alpha,
                    });
                }
            }
            Paint::LinearGradient(_) => {}
            Paint::VarLinearGradient(v) => {
                self.line(&mut v.color_line)?;
                self.field(v.var_index_base, 0, &mut v.x0)?;
                self.field(v.var_index_base, 1, &mut v.y0)?;
                self.field(v.var_index_base, 2, &mut v.x1)?;
                self.field(v.var_index_base, 3, &mut v.y1)?;
                self.field(v.var_index_base, 4, &mut v.x2)?;
                self.field(v.var_index_base, 5, &mut v.y2)?;
                if self.axes.all_pinned() {
                    *paint = Paint::LinearGradient(PaintLinearGradient {
                        color_line: static_line(&v.color_line).into(),
                        x0: v.x0,
                        y0: v.y0,
                        x1: v.x1,
                        y1: v.y1,
                        x2: v.x2,
                        y2: v.y2,
                    });
                }
            }
            Paint::RadialGradient(_) => {}
            Paint::VarRadialGradient(v) => {
                if let Some(radial) = self.radial(v)? {
                    *paint = radial;
                    return Ok(());
                }
                self.line(&mut v.color_line)?;
                self.field(v.var_index_base, 0, &mut v.x0)?;
                self.field(v.var_index_base, 1, &mut v.y0)?;
                self.field(v.var_index_base, 2, &mut v.radius0)?;
                self.field(v.var_index_base, 3, &mut v.x1)?;
                self.field(v.var_index_base, 4, &mut v.y1)?;
                self.field(v.var_index_base, 5, &mut v.radius1)?;
                if self.axes.all_pinned() {
                    *paint = Paint::RadialGradient(PaintRadialGradient {
                        color_line: static_line(&v.color_line).into(),
                        x0: v.x0,
                        y0: v.y0,
                        radius0: v.radius0,
                        x1: v.x1,
                        y1: v.y1,
                        radius1: v.radius1,
                    });
                }
            }
            Paint::SweepGradient(_) => {}
            Paint::VarSweepGradient(v) => {
                self.line(&mut v.color_line)?;
                self.field(v.var_index_base, 0, &mut v.center_x)?;
                self.field(v.var_index_base, 1, &mut v.center_y)?;
                self.field(v.var_index_base, 2, &mut v.start_angle)?;
                self.field(v.var_index_base, 3, &mut v.end_angle)?;
                if self.axes.all_pinned() {
                    *paint = Paint::SweepGradient(PaintSweepGradient {
                        color_line: static_line(&v.color_line).into(),
                        center_x: v.center_x,
                        center_y: v.center_y,
                        start_angle: v.start_angle,
                        end_angle: v.end_angle,
                    });
                }
            }
            Paint::Glyph(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::Glyph2(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::ColrGlyph(_) => {}
            Paint::Transform(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarTransform(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                let t = &mut v.transform;
                self.field(t.var_index_base, 0, &mut t.xx)?;
                self.field(t.var_index_base, 1, &mut t.yx)?;
                self.field(t.var_index_base, 2, &mut t.xy)?;
                self.field(t.var_index_base, 3, &mut t.yy)?;
                self.field(t.var_index_base, 4, &mut t.dx)?;
                self.field(t.var_index_base, 5, &mut t.dy)?;
                if self.axes.all_pinned() {
                    let t = Affine2x3 {
                        xx: t.xx,
                        yx: t.yx,
                        xy: t.xy,
                        yy: t.yy,
                        dx: t.dx,
                        dy: t.dy,
                    };
                    *paint = Paint::Transform(PaintTransform {
                        paint: v.paint.clone(),
                        transform: t.into(),
                    });
                }
            }
            Paint::Translate(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarTranslate(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                self.field(v.var_index_base, 0, &mut v.dx)?;
                self.field(v.var_index_base, 1, &mut v.dy)?;
                if self.axes.all_pinned() {
                    *paint = Paint::Translate(PaintTranslate {
                        paint: v.paint.clone(),
                        dx: v.dx,
                        dy: v.dy,
                    });
                }
            }
            Paint::Scale(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarScale(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                let sx = self.scale(v.var_index_base, 0, v.scale_x)?;
                let sy = self.scale(v.var_index_base, 1, v.scale_y)?;
                if self.axes.all_pinned() {
                    if let Some(transform) = scale_transform(&v.paint, sx, sy, 0, 0) {
                        *paint = transform;
                        return Ok(());
                    }
                }
                self.field(v.var_index_base, 0, &mut v.scale_x)?;
                self.field(v.var_index_base, 1, &mut v.scale_y)?;
                if self.axes.all_pinned() {
                    *paint = Paint::Scale(PaintScale {
                        paint: v.paint.clone(),
                        scale_x: v.scale_x,
                        scale_y: v.scale_y,
                    });
                }
            }
            Paint::ScaleAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarScaleAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                let sx = self.scale(v.var_index_base, 0, v.scale_x)?;
                let sy = self.scale(v.var_index_base, 1, v.scale_y)?;
                self.field(v.var_index_base, 2, &mut v.center_x)?;
                self.field(v.var_index_base, 3, &mut v.center_y)?;
                if self.axes.all_pinned() {
                    if let Some(transform) =
                        scale_transform(&v.paint, sx, sy, v.center_x.to_i16(), v.center_y.to_i16())
                    {
                        *paint = transform;
                        return Ok(());
                    }
                }
                self.field(v.var_index_base, 0, &mut v.scale_x)?;
                self.field(v.var_index_base, 1, &mut v.scale_y)?;
                if self.axes.all_pinned() {
                    *paint = Paint::ScaleAroundCenter(PaintScaleAroundCenter {
                        paint: v.paint.clone(),
                        scale_x: v.scale_x,
                        scale_y: v.scale_y,
                        center_x: v.center_x,
                        center_y: v.center_y,
                    });
                }
            }
            Paint::ScaleUniform(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarScaleUniform(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                let scale = self.scale(v.var_index_base, 0, v.scale)?;
                if self.axes.all_pinned() {
                    if let Some(transform) = scale_transform(&v.paint, scale, scale, 0, 0) {
                        *paint = transform;
                        return Ok(());
                    }
                }
                self.field(v.var_index_base, 0, &mut v.scale)?;
                if self.axes.all_pinned() {
                    *paint = Paint::ScaleUniform(PaintScaleUniform {
                        paint: v.paint.clone(),
                        scale: v.scale,
                    });
                }
            }
            Paint::ScaleUniformAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarScaleUniformAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                let scale = self.scale(v.var_index_base, 0, v.scale)?;
                self.field(v.var_index_base, 1, &mut v.center_x)?;
                self.field(v.var_index_base, 2, &mut v.center_y)?;
                if self.axes.all_pinned() {
                    if let Some(transform) = scale_transform(
                        &v.paint,
                        scale,
                        scale,
                        v.center_x.to_i16(),
                        v.center_y.to_i16(),
                    ) {
                        *paint = transform;
                        return Ok(());
                    }
                }
                self.field(v.var_index_base, 0, &mut v.scale)?;
                if self.axes.all_pinned() {
                    *paint = Paint::ScaleUniformAroundCenter(PaintScaleUniformAroundCenter {
                        paint: v.paint.clone(),
                        scale: v.scale,
                        center_x: v.center_x,
                        center_y: v.center_y,
                    });
                }
            }
            Paint::Rotate(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarRotate(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                self.angle(v.var_index_base, 0, &mut v.angle)?;
                if self.axes.all_pinned() {
                    *paint = Paint::Rotate(PaintRotate {
                        paint: v.paint.clone(),
                        angle: v.angle,
                    });
                }
            }
            Paint::RotateAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarRotateAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                self.angle(v.var_index_base, 0, &mut v.angle)?;
                self.field(v.var_index_base, 1, &mut v.center_x)?;
                self.field(v.var_index_base, 2, &mut v.center_y)?;
                if self.axes.all_pinned() {
                    *paint = Paint::RotateAroundCenter(PaintRotateAroundCenter {
                        paint: v.paint.clone(),
                        angle: v.angle,
                        center_x: v.center_x,
                        center_y: v.center_y,
                    });
                }
            }
            Paint::Skew(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarSkew(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                self.angle(v.var_index_base, 0, &mut v.x_skew_angle)?;
                self.angle(v.var_index_base, 1, &mut v.y_skew_angle)?;
                if self.axes.all_pinned() {
                    *paint = Paint::Skew(PaintSkew {
                        paint: v.paint.clone(),
                        x_skew_angle: v.x_skew_angle,
                        y_skew_angle: v.y_skew_angle,
                    });
                }
            }
            Paint::SkewAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
            }
            Paint::VarSkewAroundCenter(v) => {
                self.paint(&mut v.paint, depth + 1)?;
                self.angle(v.var_index_base, 0, &mut v.x_skew_angle)?;
                self.angle(v.var_index_base, 1, &mut v.y_skew_angle)?;
                self.field(v.var_index_base, 2, &mut v.center_x)?;
                self.field(v.var_index_base, 3, &mut v.center_y)?;
                if self.axes.all_pinned() {
                    *paint = Paint::SkewAroundCenter(PaintSkewAroundCenter {
                        paint: v.paint.clone(),
                        x_skew_angle: v.x_skew_angle,
                        y_skew_angle: v.y_skew_angle,
                        center_x: v.center_x,
                        center_y: v.center_y,
                    });
                }
            }
            Paint::Composite(v) => {
                self.paint(&mut v.source_paint, depth + 1)?;
                self.paint(&mut v.backdrop_paint, depth + 1)?;
            }
        }
        Ok(())
    }
}
fn nonnegative_radial(
    source: [f64; 6],
    line: ColorLine,
    source_offsets: Vec<f64>,
) -> Result<Paint, SubsetError> {
    let transparent = || {
        Paint::Solid(PaintSolid {
            palette_index: u16::MAX,
            alpha: F2Dot14::ZERO,
        })
    };
    let [x0, y0, r0, x1, y1, r1] = source;
    if r0 == r1 || source_offsets.is_empty() {
        return Ok(transparent());
    }
    let mut ordered: Vec<_> = source_offsets.into_iter().zip(line.color_stops).collect();
    ordered.sort_by(|(a, _), (b, _)| a.total_cmp(b));
    let (source_offsets, color_stops): (Vec<_>, Vec<_>) = ordered.into_iter().unzip();
    let line = ColorLine::new(line.extend, color_stops.len() as u16, color_stops);
    let dr = r1 - r0;
    // Fractional shifts must preserve both integer circles and F2Dot14 stops.
    let mut lattice = 16384_u64;
    for difference in [x1 - x0, y1 - y0, dr] {
        let mut right = difference.abs() as u64;
        while right != 0 {
            let remainder = lattice % right;
            lattice = right;
            right = remainder;
        }
    }
    let lattice = lattice as f64;
    for shift in 0..16 {
        let b = f64::from(1 << shift);
        let coarse = if dr > 0. {
            (-r0 / dr).ceil()
        } else {
            (r0 / -dr).floor() - b
        };
        let fine = if dr > 0. {
            (-r0 / dr * lattice).ceil() / lattice
        } else {
            (r0 / -dr * lattice).floor() / lattice - b
        };
        for a in [coarse, fine] {
            // omega = a + b*q. Positive b preserves the largest valid root.
            let geometry = [
                x0 + a * (x1 - x0),
                y0 + a * (y1 - y0),
                r0 + a * dr,
                x0 + (a + b) * (x1 - x0),
                y0 + (a + b) * (y1 - y0),
                r0 + (a + b) * dr,
            ];
            if [0, 1, 3, 4]
                .into_iter()
                .any(|i| !(-32768. ..=32767.).contains(&geometry[i]))
                || [2, 5]
                    .into_iter()
                    .any(|i| !(0. ..=65535.).contains(&geometry[i]))
            {
                continue;
            }
            let mut stops = line.color_stops.clone();
            let mut offsets: Vec<_> = source_offsets
                .iter()
                .map(|offset| (offset - a) / b)
                .collect();
            let first = offsets[0];
            let last = *offsets.last().unwrap();
            let tip = -geometry[2] / (b * dr);
            if line.extend == Extend::Pad {
                if dr > 0. && last <= tip {
                    stops = vec![stops.last().unwrap().clone()];
                    offsets = vec![0.];
                } else if dr < 0. && first > tip {
                    stops = vec![stops[0].clone()];
                    offsets = vec![0.];
                }
            } else {
                let mut period = last - first;
                if line.extend == Extend::Reflect {
                    period *= 2.;
                }
                if period <= 0. {
                    return Ok(transparent());
                }
                let n = ((-2. - first) / period).ceil();
                if n > ((32767. / 16384. - last) / period).floor() {
                    continue;
                }
                for offset in &mut offsets {
                    *offset += n * period;
                }
            }
            if offsets.iter().any(|offset| {
                let bits = offset * 16384.;
                !(-32768. ..=32767.).contains(&bits) || bits != bits.round()
            }) {
                continue;
            }
            for (stop, offset) in stops.iter_mut().zip(offsets) {
                stop.stop_offset = F2Dot14::from_bits((offset * 16384.) as i16);
            }
            return Ok(Paint::RadialGradient(PaintRadialGradient {
                color_line: ColorLine::new(line.extend, stops.len() as u16, stops).into(),
                x0: FWord::new(geometry[0] as i16),
                y0: FWord::new(geometry[1] as i16),
                radius0: UfWord::new(geometry[2] as u16),
                x1: FWord::new(geometry[3] as i16),
                y1: FWord::new(geometry[4] as i16),
                radius1: UfWord::new(geometry[5] as u16),
            }));
        }
    }
    Err(error())
}

fn scale_transform(paint: &Paint, sx: f64, sy: f64, cx: i16, cy: i16) -> Option<Paint> {
    if (-2. ..2.).contains(&sx) && (-2. ..2.).contains(&sy) {
        return None;
    }
    // Variation can exceed the compact scale fields' F2Dot14 range. Preserve
    // the derived scale with the wider fields of PaintTransform.
    let cx = f64::from(cx);
    let cy = f64::from(cy);
    let dx = cx * (1. - sx);
    let dy = cy * (1. - sy);
    Some(Paint::Transform(PaintTransform {
        paint: paint.clone().into(),
        transform: Affine2x3 {
            xx: Fixed::from_f64(sx),
            yx: Fixed::ZERO,
            xy: Fixed::ZERO,
            yy: Fixed::from_f64(sy),
            dx: Fixed::from_f64(dx),
            dy: Fixed::from_f64(dy),
        }
        .into(),
    }))
}
trait ColorValue: Copy {
    fn add(self, delta: f32) -> Self;
}
// Integer fields round the delta before addition. Fixed fields add it in
// sufficient precision before rounding back to the field's stored integer units.
impl ColorValue for FWord {
    fn add(self, d: f32) -> Self {
        FWord::new((self.to_i16() as i64 + super::round_f32(d) as i64).clamp(-32768, 32767) as i16)
    }
}
impl ColorValue for UfWord {
    fn add(self, d: f32) -> Self {
        UfWord::new((self.to_u16() as i64 + super::round_f32(d) as i64).clamp(0, 65535) as u16)
    }
}
impl ColorValue for F2Dot14 {
    fn add(self, d: f32) -> Self {
        Self::from_bits(
            (super::round_f32(self.to_bits() as f32 + d) as i64).clamp(-32768, 32767) as i16,
        )
    }
}
impl ColorValue for Fixed {
    fn add(self, d: f32) -> Self {
        Self::from_bits((f64::from(self.to_bits()) + f64::from(d) + 0.5).floor() as i32)
    }
}
fn static_line(v: &VarColorLine) -> ColorLine {
    ColorLine {
        extend: v.extend,
        num_stops: v.num_stops,
        color_stops: v
            .color_stops
            .iter()
            .map(|s| ColorStop {
                stop_offset: s.stop_offset,
                palette_index: s.palette_index,
                alpha: s.alpha,
            })
            .collect(),
    }
}

// Check borrowed paint offsets before recursive owned-table conversion.
fn validate_paint(
    paint: read_fonts::tables::colr::Paint,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), SubsetError> {
    use read_fonts::tables::colr::Paint;
    if depth > 64 || *remaining == 0 {
        return Err(error());
    }
    *remaining -= 1;
    macro_rules! children {($($variant:ident),*) => {
        match paint {
            $(Paint::$variant(v) => validate_paint(v.paint().map_err(|_| error())?, depth+1, remaining),)*
            Paint::Composite(v) => {
                validate_paint(v.source_paint().map_err(|_| error())?, depth+1, remaining)?;
                validate_paint(v.backdrop_paint().map_err(|_| error())?, depth+1, remaining)
            }
            _ => Ok(()),
        }
    }}
    children!(
        Glyph,
        Transform,
        VarTransform,
        Translate,
        VarTranslate,
        Scale,
        VarScale,
        ScaleAroundCenter,
        VarScaleAroundCenter,
        ScaleUniform,
        VarScaleUniform,
        ScaleUniformAroundCenter,
        VarScaleUniformAroundCenter,
        Rotate,
        VarRotate,
        RotateAroundCenter,
        VarRotateAroundCenter,
        Skew,
        VarSkew,
        SkewAroundCenter,
        VarSkewAroundCenter
    )
}
impl super::Instance<super::FontContext<'_>> for read_fonts::tables::colr::Colr<'_> {
    type Output = super::TableChange;
    fn instance(&self, context: &mut super::FontContext<'_>) -> Result<Self::Output, SubsetError> {
        use super::TableChange;
        let axes = context.axes;
        let colr = self;
        if colr.version() == 0 {
            return Ok(TableChange::Keep);
        }
        let mut remaining = 200_000;
        if let Some(list) = colr.base_glyph_list().transpose().map_err(|_| error())? {
            for record in list.base_glyph_paint_records() {
                validate_paint(
                    record.paint(list.offset_data()).map_err(|_| error())?,
                    0,
                    &mut remaining,
                )?;
            }
        }
        if let Some(list) = colr.layer_list().transpose().map_err(|_| error())? {
            for paint in list.paints().iter() {
                validate_paint(paint.map_err(|_| error())?, 0, &mut remaining)?;
            }
        }
        let c = Context {
            axes,
            store: colr
                .item_variation_store()
                .transpose()
                .map_err(|_| error())?,
            map: colr.var_index_map().transpose().map_err(|_| error())?,
            scalars: OnceCell::new(),
        };
        let mut table: Colr = colr.to_owned_table();
        if let Some(list) = table.base_glyph_list.as_mut() {
            for rec in &mut list.base_glyph_paint_records {
                c.paint(&mut rec.paint, 0)?;
            }
        }
        if let Some(list) = table.layer_list.as_mut() {
            for paint in &mut list.paints {
                c.paint(paint, 0)?;
            }
        }
        if let Some(list) = table.clip_list.as_mut() {
            for clip in &mut list.clips {
                if let ClipBox::Format2(v) = clip.clip_box.as_mut() {
                    for (i, value) in [&mut v.x_min, &mut v.y_min, &mut v.x_max, &mut v.y_max]
                        .into_iter()
                        .enumerate()
                    {
                        c.clip_edge(v.var_index_base, i as u32, value)?;
                    }
                    if axes.all_pinned() {
                        *clip.clip_box = ClipBox::Format1(ClipBoxFormat1 {
                            x_min: v.x_min,
                            y_min: v.y_min,
                            x_max: v.x_max,
                            y_max: v.y_max,
                        });
                    }
                }
            }
        }
        if axes.all_pinned() {
            table.var_index_map = Default::default();
            table.item_variation_store = Default::default();
        } else {
            let mut residual = c
                .store
                .as_ref()
                .map(|s| {
                    let mut plan = StorePlan::new(s, axes)?;
                    plan.preserve_original_region_order(s, axes)?;
                    plan.rebuild(s)
                })
                .transpose()?;
            if let (Some(store), Some(map)) = (residual.as_mut(), c.map.as_ref()) {
                let count = match map {
                    DeltaSetIndexMap::Format0(map) => u64::from(map.map_count()),
                    DeltaSetIndexMap::Format1(map) => u64::from(map.map_count()),
                };
                // Empty maps have implicit identity indices, like a missing map.
                // Keep them stable; bound allocation for explicit map rebuilding.
                if count != 0 && count <= 4_000_000 {
                    let (optimized, indices) =
                        optimize::optimize(std::mem::take(store)).map_err(|_| error())?;
                    let mapping = (0..count as u32)
                        .map(|i| {
                            let old = map.get(i).map_err(|_| error())?;
                            let old = (u32::from(old.outer) << 16) | u32::from(old.inner);
                            Ok(indices.get(&old).copied().unwrap_or(u32::MAX))
                        })
                        .collect::<Result<Vec<_>, SubsetError>>()?;
                    table.var_index_map = Some(
                        mapping
                            .into_iter()
                            .collect::<write_fonts::tables::variations::DeltaSetIndexMap>(),
                    )
                    .into();
                    *store = optimized;
                    if store.item_variation_data.is_empty() {
                        residual = None;
                    }
                }
            }
            // Without an index map, paint fields address consecutive rows directly;
            // retain those row indices instead of applying store optimization.
            table.item_variation_store = residual.map(Into::into).unwrap_or_default();
        }
        Ok(TableChange::Replace(
            write_fonts::dump_table(&table).map_err(|_| error())?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use read_fonts::{FontRef, TableProvider};
    use write_fonts::tables::variations::*;
    use write_fonts::FontBuilder;

    #[test]
    fn rounding_preserves_integral_fields_and_deltas_just_below_half() {
        let below_half = f32::from_bits(0.5f32.to_bits() - 1);
        assert_eq!(FWord::new(10).add(below_half).to_i16(), 10);
        assert_eq!(UfWord::new(10).add(below_half).to_u16(), 10);
        assert_eq!(F2Dot14::ZERO.add(below_half), F2Dot14::ZERO);
        assert_eq!(Fixed::ZERO.add(below_half), Fixed::ZERO);
        for bits in [15383929, -15383929] {
            assert_eq!(Fixed::from_bits(bits).add(0.).to_bits(), bits);
        }
        assert_eq!(FWord::new(10).add(-0.5).to_i16(), 10);
        assert_eq!(FWord::new(10).add(0.5).to_i16(), 11);
    }

    #[test]
    fn wide_deltas_and_fixed_bases_use_float_precision() {
        use read_fonts::{FontData, FontRead};
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let axes = AxisPlan::new(
            &font,
            &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
        )
        .unwrap();
        let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
        let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
        let bytes = write_fonts::dump_table(&write_fonts::tables::variations::ItemVariationStore {
            variation_region_list: VariationRegionList::new(
                2,
                vec![
                    VariationRegion::new(vec![pos.clone(), zero.clone()]),
                    VariationRegion::new(vec![zero, pos]),
                ],
            )
            .into(),
            item_variation_data: vec![ItemVariationData {
                item_count: 2,
                word_delta_count: 0x8002,
                region_indexes: vec![0, 1],
                delta_sets: [i32::MAX, i32::MAX, -i32::MAX, -i32::MAX]
                    .into_iter()
                    .flat_map(i32::to_be_bytes)
                    .collect(),
            }
            .into()],
        })
        .unwrap();
        let c = Context {
            axes: &axes,
            store: Some(
                read_fonts::tables::variations::ItemVariationStore::read(FontData::new(&bytes))
                    .unwrap(),
            ),
            map: None,
            scalars: OnceCell::new(),
        };
        let mut unchanged = Fixed::from_bits(16777217);
        c.field(u32::MAX, 0, &mut unchanged).unwrap();
        assert_eq!(unchanged.to_bits(), 16777217);
        for (index, base, expected) in [(0, i32::MIN, i32::MAX), (1, i32::MAX, i32::MIN)] {
            let mut value = Fixed::from_bits(base);
            c.field(index, 0, &mut value).unwrap();
            assert_eq!(value.to_bits(), expected);
        }
    }

    #[test]
    fn deep_paints_are_rejected_before_owned_conversion() {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let mut colr = vec![0; 34];
        colr[..2].copy_from_slice(&1u16.to_be_bytes());
        colr[14..18].copy_from_slice(&34u32.to_be_bytes());
        colr.extend(1u32.to_be_bytes());
        colr.extend(0u16.to_be_bytes());
        colr.extend(10u32.to_be_bytes());
        colr.extend([14, 0, 0, 8, 0, 0, 0, 0].repeat(4096));
        colr.extend([2, 0, 0, 0x40, 0]);
        let mut builder = FontBuilder::new();
        for r in font.table_directory().table_records() {
            builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
        }
        builder.add_raw(Tag::new(b"COLR"), colr);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        assert!(matches!(
            crate::instance_font(&font,&crate::parse_axis_limits("wght=900,CNTR=drop").unwrap()),
            Err(SubsetError::SubsetTableError(tag)) if tag == Tag::new(b"COLR")
        ));
    }
    #[test]
    fn color_values_compose_across_partial_and_full_instances() {
        let data = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&data).unwrap();
        let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
        let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
        let deltas = [
            [8192, 4096],
            [20, 10],
            [40, 20],
            [4096, 2048],
            [10, 5],
            [20, 10],
            [30, 15],
            [40, 20],
        ];
        let store = write_fonts::tables::variations::ItemVariationStore {
            variation_region_list: VariationRegionList::new(
                2,
                vec![
                    VariationRegion::new(vec![pos.clone(), zero.clone()]),
                    VariationRegion::new(vec![zero, pos]),
                ],
            )
            .into(),
            item_variation_data: vec![ItemVariationData {
                item_count: 8,
                word_delta_count: 2,
                region_indexes: vec![0, 1],
                delta_sets: deltas
                    .into_iter()
                    .flatten()
                    .flat_map(i16::to_be_bytes)
                    .collect(),
            }
            .into()],
        };
        let colr = Colr {
            base_glyph_list: BaseGlyphList {
                num_base_glyph_paint_records: 1,
                base_glyph_paint_records: vec![BaseGlyphPaint {
                    glyph_id: read_fonts::types::GlyphId16::new(1),
                    paint: Paint::VarScaleUniformAroundCenter(PaintVarScaleUniformAroundCenter {
                        paint: Paint::VarSolid(PaintVarSolid {
                            palette_index: 0,
                            alpha: F2Dot14::from_f64(0.25),
                            var_index_base: 3,
                        })
                        .into(),
                        scale: F2Dot14::ONE,
                        center_x: FWord::new(100),
                        center_y: FWord::new(200),
                        var_index_base: 0,
                    })
                    .into(),
                }],
            }
            .into(),
            clip_list: ClipList {
                format: 1,
                num_clips: 1,
                clips: vec![Clip {
                    start_glyph_id: read_fonts::types::GlyphId16::new(1),
                    end_glyph_id: read_fonts::types::GlyphId16::new(1),
                    clip_box: ClipBox::Format2(ClipBoxFormat2 {
                        x_min: FWord::new(0),
                        y_min: FWord::new(0),
                        x_max: FWord::new(500),
                        y_max: FWord::new(700),
                        var_index_base: 4,
                    })
                    .into(),
                }],
            }
            .into(),
            item_variation_store: store.into(),
            ..Default::default()
        };
        let mut builder = FontBuilder::new();
        for r in font.table_directory().table_records() {
            builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
        }
        builder.add_table(&colr).unwrap();
        let data = builder.build();
        let font = FontRef::new(&data).unwrap();
        let full = crate::instance_font(
            &font,
            &crate::parse_axis_limits("wght=900,CNTR=50").unwrap(),
        )
        .unwrap();
        let full = FontRef::new(&full).unwrap();
        let partial =
            crate::instance_font(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
        let partial = FontRef::new(&partial).unwrap();
        let store = partial
            .colr()
            .unwrap()
            .item_variation_store()
            .unwrap()
            .unwrap();
        assert_eq!(store.variation_region_list().unwrap().axis_count(), 1);
        let composed =
            crate::instance_font(&partial, &crate::parse_axis_limits("CNTR=50").unwrap()).unwrap();
        let composed = FontRef::new(&composed).unwrap();
        let a: Colr = full.colr().unwrap().to_owned_table();
        let b: Colr = composed.colr().unwrap().to_owned_table();
        assert_eq!(a, b);
        assert!(a.item_variation_store.is_none());
        let Paint::ScaleUniformAroundCenter(p) =
            a.base_glyph_list.as_ref().unwrap().base_glyph_paint_records[0]
                .paint
                .as_ref()
        else {
            panic!()
        };
        assert_eq!(p.center_x, FWord::new(125));
        assert_eq!(p.scale, F2Dot14::from_f64(1.625));
        let Paint::Solid(s) = p.paint.as_ref() else {
            panic!()
        };
        assert_eq!(s.alpha, F2Dot14::from_f64(0.5625));
    }
}
