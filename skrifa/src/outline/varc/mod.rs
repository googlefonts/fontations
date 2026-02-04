//! Support for rendering variable composite glyphs from the VARC table.

use read_fonts::{
    tables::{
        layout::Condition,
        varc::{
            DecomposedTransform, MultiItemVariationStore, SparseVariationRegion, Varc,
            VarcComponent, VarcFlags,
        },
        variations::NO_VARIATION_INDEX,
    },
    types::{F2Dot14, GlyphId},
    FontRef, ReadError, TableProvider,
};

use crate::{
    collections::SmallVec,
    instance::{LocationRef, Size},
    outline::{
        cff, glyf, metrics::GlyphHMetrics, pen::PathStyle, DrawError, DrawSettings, OutlinePen,
    },
    provider::MetadataProvider,
    GLYF_COMPOSITE_RECURSION_LIMIT,
};

use super::{OutlineGlyph, OutlineKind};

type GlyphStack = SmallVec<GlyphId, 8>;

#[derive(Clone)]
enum BaseOutlines<'a> {
    Glyf(glyf::Outlines<'a>),
    Cff(cff::Outlines<'a>),
}

impl<'a> BaseOutlines<'a> {
    fn glyph_count(&self) -> u32 {
        match self {
            Self::Glyf(glyf) => glyf.glyph_count() as u32,
            Self::Cff(cff) => cff.glyph_count() as u32,
        }
    }

    fn prefer_interpreter(&self) -> bool {
        match self {
            Self::Glyf(glyf) => glyf.prefer_interpreter(),
            _ => false,
        }
    }

    fn fractional_size_hinting(&self) -> bool {
        match self {
            Self::Glyf(glyf) => glyf.fractional_size_hinting,
            _ => true,
        }
    }

    fn font(&self) -> &FontRef<'a> {
        match self {
            Self::Glyf(glyf) => &glyf.font,
            Self::Cff(cff) => &cff.font,
        }
    }

    fn base_outline_kind(&self, glyph_id: GlyphId) -> Option<OutlineKind<'a>> {
        match self {
            Self::Glyf(glyf) => Some(OutlineKind::Glyf(
                glyf.clone(),
                glyf.outline(glyph_id).ok()?,
            )),
            Self::Cff(cff) => Some(OutlineKind::Cff(
                cff.clone(),
                glyph_id,
                cff.subfont_index(glyph_id),
            )),
        }
    }

    fn base_outline_memory(&self, glyph_id: GlyphId) -> usize {
        match self {
            Self::Glyf(glyf) => glyf
                .outline(glyph_id)
                .ok()
                .map(|outline| outline.required_buffer_size(super::Hinting::None))
                .unwrap_or(0),
            Self::Cff(..) => 0,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Outlines<'a> {
    varc: Varc<'a>,
    base: BaseOutlines<'a>,
    glyph_metrics: GlyphHMetrics<'a>,
    units_per_em: u16,
    axis_count: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Outline {
    pub(crate) glyph_id: GlyphId,
    pub(crate) coverage_index: u16,
    max_component_memory: usize,
}

impl Outline {
    pub fn required_buffer_size(&self) -> usize {
        self.max_component_memory
    }
}

impl<'a> Outlines<'a> {
    pub fn new(font: &FontRef<'a>) -> Option<Self> {
        let varc = font.varc().ok()?;
        if let Some(glyf) = glyf::Outlines::new(font) {
            return Self::from_base(font, varc, BaseOutlines::Glyf(glyf));
        }
        if let Some(cff) = cff::Outlines::new(font) {
            return Self::from_base(font, varc, BaseOutlines::Cff(cff));
        }
        None
    }

    fn from_base(font: &FontRef<'a>, varc: Varc<'a>, base: BaseOutlines<'a>) -> Option<Self> {
        let glyph_metrics = GlyphHMetrics::new(font)?;
        let units_per_em = font.head().ok()?.units_per_em();
        let axis_count = font.axes().len();
        Some(Self {
            varc,
            base,
            glyph_metrics,
            units_per_em,
            axis_count,
        })
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn glyph_count(&self) -> u32 {
        self.base.glyph_count()
    }

    pub fn prefer_interpreter(&self) -> bool {
        self.base.prefer_interpreter()
    }

    pub fn fractional_size_hinting(&self) -> bool {
        self.base.fractional_size_hinting()
    }

    pub fn font(&self) -> &FontRef<'a> {
        self.base.font()
    }

    pub fn outline(&self, glyph_id: GlyphId) -> Result<Option<Outline>, ReadError> {
        let coverage = self.varc.coverage()?;
        let Some(coverage_index) = coverage.get(glyph_id) else {
            return Ok(None);
        };
        let max_component_memory = self.compute_max_component_memory(glyph_id, coverage_index)?;
        Ok(Some(Outline {
            glyph_id,
            coverage_index,
            max_component_memory,
        }))
    }

    fn compute_max_component_memory(
        &self,
        glyph_id: GlyphId,
        coverage_index: u16,
    ) -> Result<usize, ReadError> {
        let mut stack = GlyphStack::new();
        self.max_component_memory_for_glyph(glyph_id, coverage_index, &mut stack)
    }

    fn max_component_memory_for_glyph(
        &self,
        glyph_id: GlyphId,
        coverage_index: u16,
        stack: &mut GlyphStack,
    ) -> Result<usize, ReadError> {
        if stack.contains(&glyph_id) {
            return Ok(0);
        }
        stack.push(glyph_id);
        let mut max_memory = 0usize;
        let glyph = self.varc.glyph(coverage_index as usize)?;
        for component in glyph.components() {
            let component = component?;
            let component_gid = component.gid();
            let component_memory = if component_gid == glyph_id {
                self.base.base_outline_memory(component_gid)
            } else if let Some(component_outline) = self.outline(component_gid)? {
                self.max_component_memory_for_glyph(
                    component_gid,
                    component_outline.coverage_index,
                    stack,
                )?
            } else {
                self.base.base_outline_memory(component_gid)
            };
            max_memory = max_memory.max(component_memory);
        }
        stack.pop();
        Ok(max_memory)
    }

    pub fn draw(
        &self,
        outline: &Outline,
        buf: &mut [u8],
        size: Size,
        coords: &[F2Dot14],
        path_style: PathStyle,
        pen: &mut impl OutlinePen,
    ) -> Result<(), DrawError> {
        let font_coords = expand_coords(self.axis_count, coords);
        let mut stack = GlyphStack::new();
        let pen: &mut dyn OutlinePen = pen;
        self.draw_glyph(
            outline.glyph_id,
            outline.coverage_index,
            &font_coords,
            &font_coords,
            size,
            path_style,
            buf,
            pen,
            &mut stack,
            0,
        )
    }

    pub fn draw_unscaled(
        &self,
        outline: &Outline,
        buf: &mut [u8],
        coords: &[F2Dot14],
        pen: &mut impl OutlinePen,
    ) -> Result<i32, DrawError> {
        let size = Size::unscaled();
        self.draw(outline, buf, size, coords, PathStyle::default(), pen)?;
        Ok(self.glyph_metrics.advance_width(outline.glyph_id, coords))
    }

    fn draw_glyph(
        &self,
        glyph_id: GlyphId,
        coverage_index: u16,
        font_coords: &[F2Dot14],
        current_coords: &[F2Dot14],
        size: Size,
        path_style: PathStyle,
        buf: &mut [u8],
        pen: &mut dyn OutlinePen,
        stack: &mut GlyphStack,
        depth: usize,
    ) -> Result<(), DrawError> {
        if depth > GLYF_COMPOSITE_RECURSION_LIMIT {
            return Err(DrawError::RecursionLimitExceeded(glyph_id));
        }
        if stack.contains(&glyph_id) {
            return Ok(());
        }
        let mut scalar_cache = self.scalar_cache()?;
        let glyph = self.varc.glyph(coverage_index as usize)?;
        stack.push(glyph_id);
        for component in glyph.components() {
            let component = component?;
            if !self.component_condition_met(&component, current_coords, scalar_cache.as_mut())? {
                continue;
            }
            let component_gid = component.gid();
            let component_coords = self.component_coords(
                &component,
                font_coords,
                current_coords,
                scalar_cache.as_mut(),
            )?;
            let mut transform = *component.transform();
            self.apply_transform_variations(
                &component,
                current_coords,
                &mut transform,
                scalar_cache.as_mut(),
            )?;
            let matrix = matrix_with_scale(&transform, size, self.units_per_em);
            let mut transform_pen = TransformPen::new(pen, matrix);
            if component_gid != glyph_id {
                if let Some(component_outline) = self.outline(component_gid)? {
                    if !stack.contains(&component_gid) {
                        self.draw_glyph(
                            component_gid,
                            component_outline.coverage_index,
                            font_coords,
                            &component_coords,
                            size,
                            path_style,
                            buf,
                            &mut transform_pen,
                            stack,
                            depth + 1,
                        )?;
                        continue;
                    }
                }
            }
            self.draw_base_glyph(
                component_gid,
                &component_coords,
                size,
                path_style,
                buf,
                &mut transform_pen,
            )?;
        }
        stack.pop();
        Ok(())
    }

    fn draw_base_glyph(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        size: Size,
        path_style: PathStyle,
        buf: &mut [u8],
        pen: &mut impl OutlinePen,
    ) -> Result<(), DrawError> {
        let Some(kind) = self.base.base_outline_kind(glyph_id) else {
            return Err(DrawError::GlyphNotFound(glyph_id));
        };
        let glyph = OutlineGlyph { kind };
        let settings = DrawSettings::unhinted(size, LocationRef::new(coords))
            .with_path_style(path_style)
            .with_memory(Some(buf));
        glyph.draw(settings, pen)?;
        Ok(())
    }

    fn component_coords(
        &self,
        component: &VarcComponent<'a>,
        font_coords: &[F2Dot14],
        current_coords: &[F2Dot14],
        scalar_cache: Option<&mut ScalarCache>,
    ) -> Result<SmallVec<F2Dot14, 64>, DrawError> {
        let mut coords = if component
            .flags()
            .contains(VarcFlags::RESET_UNSPECIFIED_AXES)
        {
            expand_coords(font_coords.len(), font_coords)
        } else {
            expand_coords(current_coords.len(), current_coords)
        };

        if !component.flags().contains(VarcFlags::HAVE_AXES) {
            return Ok(coords);
        }

        let axis_indices_index = component
            .axis_indices_index()
            .ok_or(ReadError::MalformedData("Missing axisIndicesIndex"))?;
        let axis_indices = self.axis_indices(axis_indices_index as usize)?;
        let mut axis_values = self.axis_values(component, axis_indices.len())?;
        if let Some(var_idx) = component.axis_values_var_index() {
            let deltas = self
                .var_store()?
                .ok_or(ReadError::NullOffset)
                .and_then(|store| {
                    compute_tuple_deltas(
                        &store,
                        var_idx,
                        current_coords,
                        axis_indices.len(),
                        scalar_cache,
                    )
                })?;
            for (value, delta) in axis_values.iter_mut().zip(deltas) {
                *value += delta as f32 / 16384.0;
            }
        }

        for (axis_index, value) in axis_indices.iter().zip(axis_values) {
            let Some(slot) = coords.get_mut(*axis_index as usize) else {
                return Err(DrawError::Read(ReadError::OutOfBounds));
            };
            *slot = F2Dot14::from_f32(value);
        }
        Ok(coords)
    }

    fn axis_indices(&self, nth: usize) -> Result<SmallVec<u16, 64>, DrawError> {
        let packed = self.varc.axis_indices(nth)?;
        let mut indices = SmallVec::new();
        for value in packed.iter() {
            if value < 0 || value > u16::MAX as i32 {
                return Err(DrawError::Read(ReadError::MalformedData(
                    "axis index out of range",
                )));
            }
            indices.push(value as u16);
        }
        Ok(indices)
    }

    fn axis_values(
        &self,
        component: &VarcComponent<'a>,
        count: usize,
    ) -> Result<SmallVec<f32, 64>, DrawError> {
        let Some(packed) = component.axis_values() else {
            return Ok(SmallVec::new());
        };
        let mut values = SmallVec::new();
        let mut iter = packed.iter();
        for _ in 0..count {
            let value = iter.next().ok_or(ReadError::OutOfBounds)?;
            values.push(value as f32 / 16384.0);
        }
        Ok(values)
    }

    fn apply_transform_variations(
        &self,
        component: &VarcComponent<'a>,
        coords: &[F2Dot14],
        transform: &mut DecomposedTransform,
        scalar_cache: Option<&mut ScalarCache>,
    ) -> Result<(), DrawError> {
        let Some(var_idx) = component.transform_var_index() else {
            return Ok(());
        };

        let flags = component.flags();
        let mut fields = SmallVec::<TransformField, 9>::new();
        if flags.contains(VarcFlags::HAVE_TRANSLATE_X) {
            fields.push(TransformField::TranslateX);
        }
        if flags.contains(VarcFlags::HAVE_TRANSLATE_Y) {
            fields.push(TransformField::TranslateY);
        }
        if flags.contains(VarcFlags::HAVE_ROTATION) {
            fields.push(TransformField::Rotation);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_X) {
            fields.push(TransformField::ScaleX);
        }
        let scale_y_present = flags.contains(VarcFlags::HAVE_SCALE_Y);
        if scale_y_present {
            fields.push(TransformField::ScaleY);
        }
        if flags.contains(VarcFlags::HAVE_SKEW_X) {
            fields.push(TransformField::SkewX);
        }
        if flags.contains(VarcFlags::HAVE_SKEW_Y) {
            fields.push(TransformField::SkewY);
        }
        if flags.contains(VarcFlags::HAVE_TCENTER_X) {
            fields.push(TransformField::CenterX);
        }
        if flags.contains(VarcFlags::HAVE_TCENTER_Y) {
            fields.push(TransformField::CenterY);
        }

        if fields.is_empty() {
            return Ok(());
        }

        let deltas = self
            .var_store()?
            .ok_or(ReadError::NullOffset)
            .and_then(|store| {
                compute_tuple_deltas(&store, var_idx, coords, fields.len(), scalar_cache)
            })?;

        for (field, delta) in fields.into_iter().zip(deltas) {
            match field {
                TransformField::TranslateX => {
                    transform.set_translate_x(transform.translate_x() + delta as f64)
                }
                TransformField::TranslateY => {
                    transform.set_translate_y(transform.translate_y() + delta as f64)
                }
                TransformField::Rotation => {
                    transform.set_rotation(transform.rotation() + delta as f64 / 4096.0)
                }
                TransformField::ScaleX => {
                    transform.set_scale_x(transform.scale_x() + delta as f64 / 1024.0)
                }
                TransformField::ScaleY => {
                    transform.set_scale_y(transform.scale_y() + delta as f64 / 1024.0)
                }
                TransformField::SkewX => {
                    transform.set_skew_x(transform.skew_x() + delta as f64 / 4096.0)
                }
                TransformField::SkewY => {
                    transform.set_skew_y(transform.skew_y() + delta as f64 / 4096.0)
                }
                TransformField::CenterX => {
                    transform.set_center_x(transform.center_x() + delta as f64)
                }
                TransformField::CenterY => {
                    transform.set_center_y(transform.center_y() + delta as f64)
                }
            }
        }

        if !scale_y_present {
            transform.set_scale_y(transform.scale_x());
        }
        Ok(())
    }

    fn component_condition_met(
        &self,
        component: &VarcComponent<'a>,
        coords: &[F2Dot14],
        scalar_cache: Option<&mut ScalarCache>,
    ) -> Result<bool, DrawError> {
        let Some(condition_index) = component.condition_index() else {
            return Ok(true);
        };
        let Some(condition_list) = self.varc.condition_list() else {
            return Err(DrawError::Read(ReadError::NullOffset));
        };
        let condition_list = condition_list?;
        let condition = condition_list.conditions().get(condition_index as usize)?;
        self.eval_condition(&condition, coords, scalar_cache)
    }

    fn eval_condition(
        &self,
        condition: &Condition<'a>,
        coords: &[F2Dot14],
        mut scalar_cache: Option<&mut ScalarCache>,
    ) -> Result<bool, DrawError> {
        match condition {
            Condition::Format1AxisRange(condition) => {
                let axis_index = condition.axis_index() as usize;
                let coord = coords
                    .get(axis_index)
                    .copied()
                    .unwrap_or(F2Dot14::ZERO)
                    .to_f32();
                Ok(coord >= condition.filter_range_min_value().to_f32()
                    && coord <= condition.filter_range_max_value().to_f32())
            }
            Condition::Format2VariableValue(condition) => {
                let default_value = condition.default_value() as i32;
                let var_idx = condition.var_index();
                let delta = self
                    .var_store()?
                    .ok_or(ReadError::NullOffset)
                    .and_then(|store| {
                        compute_tuple_deltas(&store, var_idx, coords, 1, scalar_cache)
                    })?
                    .first()
                    .copied()
                    .unwrap_or(0);
                Ok(default_value + delta > 0)
            }
            Condition::Format3And(condition) => {
                for nested in condition.conditions().iter() {
                    let nested = nested?;
                    if !self.eval_condition(&nested, coords, scalar_cache.as_deref_mut())? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Condition::Format4Or(condition) => {
                for nested in condition.conditions().iter() {
                    let nested = nested?;
                    if self.eval_condition(&nested, coords, scalar_cache.as_deref_mut())? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Condition::Format5Negate(condition) => {
                let nested = condition.condition()?;
                Ok(!self.eval_condition(&nested, coords, scalar_cache)?)
            }
        }
    }

    fn var_store(&self) -> Result<Option<MultiItemVariationStore<'a>>, ReadError> {
        self.varc.multi_var_store().transpose()
    }

    fn scalar_cache(&self) -> Result<Option<ScalarCache>, DrawError> {
        let Some(store) = self.var_store()? else {
            return Ok(None);
        };
        let region_count = store.region_list()?.regions().len();
        Ok(Some(ScalarCache::new(region_count)))
    }
}

#[derive(Copy, Clone, Default)]
enum TransformField {
    #[default]
    TranslateX,
    TranslateY,
    Rotation,
    ScaleX,
    ScaleY,
    SkewX,
    SkewY,
    CenterX,
    CenterY,
}

struct ScalarCache {
    values: SmallVec<f32, 64>,
}

impl ScalarCache {
    const INVALID: f32 = f32::NAN;

    fn new(count: usize) -> Self {
        Self {
            values: SmallVec::with_len(count, Self::INVALID),
        }
    }

    fn get(&self, index: usize) -> Option<f32> {
        let Some(value) = self.values.get(index).copied() else {
            return Some(0.0);
        };
        if value == 0.0 {
            return Some(0.0);
        }
        if value.is_nan() {
            return None;
        }
        Some(value)
    }

    fn set(&mut self, index: usize, value: f32) {
        let Some(slot) = self.values.get_mut(index) else {
            return;
        };
        *slot = value;
    }
}

fn expand_coords(axis_count: usize, coords: &[F2Dot14]) -> SmallVec<F2Dot14, 64> {
    let mut out = SmallVec::with_len(axis_count, F2Dot14::ZERO);
    for (slot, value) in out.iter_mut().zip(coords.iter().copied()) {
        *slot = value;
    }
    out
}

fn compute_tuple_deltas<'a>(
    store: &MultiItemVariationStore<'a>,
    var_idx: u32,
    coords: &[F2Dot14],
    tuple_len: usize,
    mut scalar_cache: Option<&mut ScalarCache>,
) -> Result<SmallVec<i32, 32>, ReadError> {
    if tuple_len == 0 {
        return Ok(SmallVec::new());
    }
    if var_idx == NO_VARIATION_INDEX {
        return Ok(SmallVec::with_len(tuple_len, 0));
    }
    let outer = (var_idx >> 16) as usize;
    let inner = (var_idx & 0xFFFF) as usize;
    let data = store
        .variation_data()
        .get(outer)
        .map_err(|_| ReadError::InvalidCollectionIndex(outer as _))?;
    let region_indices = data.region_indices();
    let mut deltas_iter = data.delta_set(inner)?.iter();

    let regions = store.region_list()?.regions();
    let mut out = SmallVec::with_len(tuple_len, 0i32);
    if tuple_len == 1 {
        for region_index in region_indices.iter() {
            let region_idx = region_index.get() as usize;
            let region = regions.get(region_idx)?;
            let scalar = if let Some(cache) = scalar_cache.as_deref_mut() {
                if let Some(value) = cache.get(region_idx) {
                    value
                } else {
                    let value = compute_sparse_region_scalar(&region, coords);
                    cache.set(region_idx, value);
                    value
                }
            } else {
                compute_sparse_region_scalar(&region, coords)
            };
            if scalar == 0.0 {
                deltas_iter = deltas_iter.skip_fast(1);
                continue;
            }
            let delta = deltas_iter.next().ok_or(ReadError::OutOfBounds)?;
            if scalar == 1.0 {
                out[0] += delta;
            } else {
                out[0] += (delta as f32 * scalar) as i32;
            }
        }
        return Ok(out);
    }
    for region_index in region_indices.iter() {
        let region_idx = region_index.get() as usize;
        let region = regions.get(region_idx)?;
        let scalar = if let Some(cache) = scalar_cache.as_deref_mut() {
            if let Some(value) = cache.get(region_idx) {
                value
            } else {
                let value = compute_sparse_region_scalar(&region, coords);
                cache.set(region_idx, value);
                value
            }
        } else {
            compute_sparse_region_scalar(&region, coords)
        };
        if scalar == 0.0 {
            deltas_iter = deltas_iter.skip_fast(tuple_len);
            continue;
        }
        if scalar == 1.0 {
            for value in out.iter_mut() {
                let delta = deltas_iter.next().ok_or(ReadError::OutOfBounds)?;
                *value += delta;
            }
            continue;
        }
        for value in out.iter_mut() {
            let delta = deltas_iter.next().ok_or(ReadError::OutOfBounds)?;
            *value += (delta as f32 * scalar) as i32;
        }
    }

    Ok(out)
}

fn compute_sparse_region_scalar(region: &SparseVariationRegion<'_>, coords: &[F2Dot14]) -> f32 {
    let mut scalar = 1.0f32;
    for axis in region.region_axis_offsets() {
        let peak = axis.peak();
        if peak == F2Dot14::ZERO {
            continue;
        }
        let axis_index = axis.axis_index() as usize;
        let coord = coords.get(axis_index).copied().unwrap_or(F2Dot14::ZERO);
        if coord == F2Dot14::ZERO {
            return 0.0;
        }
        let start = axis.start();
        let end = axis.end();
        if start > peak || peak > end || (start < F2Dot14::ZERO && end > F2Dot14::ZERO) {
            continue;
        }
        if coord < start || coord > end {
            return 0.0;
        } else if coord == peak {
            continue;
        } else if coord < peak {
            let coord = coord.to_f32();
            let start = start.to_f32();
            let peak = peak.to_f32();
            scalar = (scalar * (coord - start)) / (peak - start);
        } else {
            let coord = coord.to_f32();
            let end = end.to_f32();
            let peak = peak.to_f32();
            scalar = (scalar * (end - coord)) / (end - peak);
        }
    }
    scalar
}

fn matrix_with_scale(transform: &DecomposedTransform, size: Size, units_per_em: u16) -> [f32; 6] {
    let mut matrix = transform.matrix();
    let scale = size.linear_scale(units_per_em) as f64;
    matrix[4] *= scale;
    matrix[5] *= scale;
    [
        matrix[0] as f32,
        matrix[1] as f32,
        matrix[2] as f32,
        matrix[3] as f32,
        matrix[4] as f32,
        matrix[5] as f32,
    ]
}

struct TransformPen<'a, P: OutlinePen + ?Sized> {
    pen: &'a mut P,
    matrix: [f32; 6],
}

impl<'a, P: OutlinePen + ?Sized> TransformPen<'a, P> {
    fn new(pen: &'a mut P, matrix: [f32; 6]) -> Self {
        Self { pen, matrix }
    }

    fn transform(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.matrix;
        (a * x + c * y + e, b * x + d * y + f)
    }
}

impl<P: OutlinePen + ?Sized> OutlinePen for TransformPen<'_, P> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform(x, y);
        self.pen.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform(x, y);
        self.pen.line_to(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (cx0, cy0) = self.transform(cx0, cy0);
        let (x, y) = self.transform(x, y);
        self.pen.quad_to(cx0, cy0, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (cx0, cy0) = self.transform(cx0, cy0);
        let (cx1, cy1) = self.transform(cx1, cy1);
        let (x, y) = self.transform(x, y);
        self.pen.curve_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.pen.close();
    }
}
