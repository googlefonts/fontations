//! Support for rendering variable composite glyphs from the VARC table.

use read_fonts::{
    tables::{
        layout::Condition,
        varc::{
            DecomposedTransform, MultiItemVariationStore, SparseVariationRegion,
            SparseVariationRegionList, Varc, VarcComponent, VarcFlags,
        },
        variations::NO_VARIATION_INDEX,
    },
    types::{F2Dot14, GlyphId},
    FontRef, ReadError, TableProvider,
};

use crate::{
    collections::SmallVec,
    instance::Size,
    outline::{cff, glyf, metrics::GlyphHMetrics, pen::PathStyle, DrawError, OutlinePen},
    provider::MetadataProvider,
    GLYF_COMPOSITE_RECURSION_LIMIT,
};

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::CoreFloat;

use super::OutlineKind;

type GlyphStack = SmallVec<GlyphId, 8>;
type CoordVec = SmallVec<F2Dot14, 64>;
type AxisIndexVec = SmallVec<u16, 64>;
type AxisValueVec = SmallVec<f32, 64>;
type DeltaVec = SmallVec<f32, 64>;
type ScalarCacheVec = SmallVec<f32, 128>;
type Affine = [f32; 6];

struct Scratchpad {
    deltas: DeltaVec,
    axis_indices: AxisIndexVec,
    axis_values: AxisValueVec,
}

impl Scratchpad {
    fn new() -> Self {
        Self {
            deltas: DeltaVec::new(),
            axis_indices: AxisIndexVec::new(),
            axis_values: AxisValueVec::new(),
        }
    }
}

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

    pub(crate) fn fallback_outline_kind(&self, glyph_id: GlyphId) -> Option<OutlineKind<'a>> {
        self.base.base_outline_kind(glyph_id)
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

    /// Lightweight coverage lookup without computing max_component_memory.
    fn coverage_index(&self, glyph_id: GlyphId) -> Result<Option<u16>, ReadError> {
        let coverage = self.varc.coverage()?;
        Ok(coverage.get(glyph_id))
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
        if stack.len() >= GLYF_COMPOSITE_RECURSION_LIMIT {
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
            } else if let Some(coverage_index) = self.coverage_index(component_gid)? {
                self.max_component_memory_for_glyph(component_gid, coverage_index, stack)?
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
        let mut font_coords = CoordVec::new();
        expand_coords(&mut font_coords, self.axis_count, coords);
        let mut stack = GlyphStack::new();
        let pen: &mut dyn OutlinePen = pen;
        let coverage = self.varc.coverage()?;
        let var_store = self.varc.multi_var_store().transpose()?;
        let regions = var_store.as_ref().map(|s| s.region_list()).transpose()?;
        let mut scalar_cache = self.scalar_cache_from_store(var_store.as_ref())?.unwrap();
        let mut scratch = Scratchpad::new();
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
            IDENTITY_MATRIX,
            var_store.as_ref(),
            regions.as_ref(),
            &coverage,
            &mut scalar_cache,
            &mut scratch,
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

    #[allow(clippy::too_many_arguments)]
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
        parent_matrix: Affine,
        var_store: Option<&MultiItemVariationStore<'a>>,
        regions: Option<&SparseVariationRegionList<'a>>,
        coverage: &read_fonts::tables::layout::CoverageTable<'a>,
        scalar_cache: &mut ScalarCache,
        scratch: &mut Scratchpad,
    ) -> Result<(), DrawError> {
        if stack.len() >= GLYF_COMPOSITE_RECURSION_LIMIT {
            return Err(DrawError::RecursionLimitExceeded(glyph_id));
        }
        let glyph = self.varc.glyph(coverage_index as usize)?;
        stack.push(glyph_id);
        let mut component_coords_buffer = CoordVec::new();
        let mut child_scalar_cache: Option<ScalarCache> = None;
        for component in glyph.components() {
            let component = component?;
            if !self.component_condition_met(
                &component,
                current_coords,
                var_store,
                regions,
                scalar_cache,
                scratch,
            )? {
                continue;
            }
            let component_gid = component.gid();
            let flags = component.flags();

            let coords_the_same = !flags.contains(VarcFlags::HAVE_AXES)
                && !flags.contains(VarcFlags::RESET_UNSPECIFIED_AXES);

            let component_coords = if coords_the_same {
                current_coords
            } else {
                self.component_coords(
                    &component,
                    font_coords,
                    current_coords,
                    var_store,
                    regions,
                    scalar_cache,
                    &mut component_coords_buffer,
                    scratch,
                )?;
                component_coords_buffer.as_slice()
            };

            let mut transform = *component.transform();
            self.apply_transform_variations(
                &component,
                current_coords,
                &mut transform,
                var_store,
                regions,
                scalar_cache,
                &mut scratch.deltas,
            )?;
            let scale = size.linear_scale(self.units_per_em);
            let matrix = mul_matrix(parent_matrix, scale_matrix(transform.matrix(), scale));
            if component_gid != glyph_id {
                if let Some(coverage_index) = coverage.get(component_gid) {
                    if !stack.contains(&component_gid) {
                        // Optimization: if coordinates haven't changed, we can reuse the scalar cache.
                        if coords_the_same {
                            self.draw_glyph(
                                component_gid,
                                coverage_index,
                                font_coords,
                                current_coords,
                                size,
                                path_style,
                                buf,
                                pen,
                                stack,
                                matrix,
                                var_store,
                                regions,
                                coverage,
                                scalar_cache,
                                scratch,
                            )?;
                        } else {
                            if let Some(ref mut cache) = child_scalar_cache {
                                cache.values.fill(ScalarCache::INVALID);
                            } else {
                                child_scalar_cache = self.scalar_cache_from_store(var_store)?;
                            }
                            self.draw_glyph(
                                component_gid,
                                coverage_index,
                                font_coords,
                                component_coords,
                                size,
                                path_style,
                                buf,
                                pen,
                                stack,
                                matrix,
                                var_store,
                                regions,
                                coverage,
                                child_scalar_cache.as_mut().unwrap(),
                                scratch,
                            )?;
                        }
                        continue;
                    }
                }
            }
            let mut transform_pen = TransformPen::new(pen, matrix);
            self.draw_base_glyph(
                component_gid,
                component_coords,
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
        let settings =
            crate::outline::DrawSettings::unhinted(size, crate::instance::LocationRef::new(coords))
                .with_path_style(path_style)
                .with_memory(Some(buf));
        crate::outline::OutlineGlyph { kind }.draw(settings, pen)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn component_coords(
        &self,
        component: &VarcComponent<'a>,
        font_coords: &[F2Dot14],
        current_coords: &[F2Dot14],
        var_store: Option<&MultiItemVariationStore<'a>>,
        regions: Option<&SparseVariationRegionList<'a>>,
        scalar_cache: &mut ScalarCache,
        coords: &mut CoordVec,
        scratch: &mut Scratchpad,
    ) -> Result<(), DrawError> {
        let flags = component.flags();
        if flags.contains(VarcFlags::RESET_UNSPECIFIED_AXES) {
            expand_coords(coords, font_coords.len(), font_coords);
        } else {
            expand_coords(coords, current_coords.len(), current_coords);
        }

        if !flags.contains(VarcFlags::HAVE_AXES) {
            return Ok(());
        }

        let axis_indices_index = component
            .axis_indices_index()
            .ok_or(ReadError::MalformedData("Missing axisIndicesIndex"))?;
        let num_axes = self.axis_indices(axis_indices_index as usize, &mut scratch.axis_indices)?;
        //println!("reset {}", flags.contains(VarcFlags::RESET_UNSPECIFIED_AXES) as u32);
        // //print axis coords as integer
        //print!("axis[");
        for (i, _coord) in current_coords.iter().copied().enumerate() {
            if i != 0 {
                //print!(", ");
            }
            //print!("{}:{}", i, (coord.to_bits()) as i32);
        }
        //println!("]");

        self.axis_values(component, num_axes, &mut scratch.axis_values)?;
        //print!("bef [");
        for (i, _value) in scratch.axis_values.iter().copied().enumerate() {
            if i != 0 {
                //print!(", ");
            }
            //print!("{}", (value * 16384.0).round() as i32);
        }
        //println!("]");
        if let Some(var_idx) = component.axis_values_var_index() {
            let store = var_store.ok_or(ReadError::NullOffset)?;
            let regions = regions.ok_or(ReadError::NullOffset)?;
            accumulate_tuple_deltas_in_place(
                store,
                regions,
                var_idx,
                current_coords,
                scratch.axis_indices.len(),
                scalar_cache,
                scratch.axis_values.as_mut_slice(),
            )?;
        }

        for (axis_index, value) in scratch
            .axis_indices
            .iter()
            .zip(scratch.axis_values.iter().copied())
        {
            //println!("Setting axis {} to {}", axis_index, (value * 16384.0).round() as i32);
            let Some(slot) = coords.get_mut(*axis_index as usize) else {
                return Err(DrawError::Read(ReadError::OutOfBounds));
            };
            let raw = value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            *slot = F2Dot14::from_bits(raw);
        }
        // //print axis values as integer
        //print!("aft [");
        for (i, _value) in scratch.axis_values.iter().copied().enumerate() {
            if i != 0 {
                //print!(", ");
            }
            //print!("{}", (value * 16384.0).round() as i32);
        }
        //println!("]");
        Ok(())
    }

    fn axis_indices(&self, nth: usize, out: &mut AxisIndexVec) -> Result<usize, DrawError> {
        let packed = self.varc.axis_indices(nth)?;
        out.clear();
        for value in packed.iter() {
            out.push(value as u16);
        }
        Ok(out.len())
    }

    fn axis_values(
        &self,
        component: &VarcComponent<'a>,
        count: usize,
        out: &mut AxisValueVec,
    ) -> Result<(), DrawError> {
        let Some(packed) = component.axis_values() else {
            out.clear();
            return Ok(());
        };
        out.resize_and_fill(count, 0.0);
        for (slot, value) in out.iter_mut().zip(packed.iter().by_ref().take(count)) {
            *slot = value as f32;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_transform_variations(
        &self,
        component: &VarcComponent<'a>,
        coords: &[F2Dot14],
        transform: &mut DecomposedTransform,
        var_store: Option<&MultiItemVariationStore<'a>>,
        regions: Option<&SparseVariationRegionList<'a>>,
        scalar_cache: &mut ScalarCache,
        deltas: &mut DeltaVec,
    ) -> Result<(), DrawError> {
        let Some(var_idx) = component.transform_var_index() else {
            return Ok(());
        };

        let flags = component.flags();

        // Count transform fields using a mask + count_ones
        const TRANSFORM_MASK: VarcFlags = VarcFlags::from_bits_truncate(
            VarcFlags::HAVE_TRANSLATE_X.bits()
                | VarcFlags::HAVE_TRANSLATE_Y.bits()
                | VarcFlags::HAVE_ROTATION.bits()
                | VarcFlags::HAVE_SCALE_X.bits()
                | VarcFlags::HAVE_SCALE_Y.bits()
                | VarcFlags::HAVE_SKEW_X.bits()
                | VarcFlags::HAVE_SKEW_Y.bits()
                | VarcFlags::HAVE_TCENTER_X.bits()
                | VarcFlags::HAVE_TCENTER_Y.bits(),
        );
        let field_count = (flags.bits() & TRANSFORM_MASK.bits()).count_ones() as usize;
        if field_count == 0 {
            return Ok(());
        }

        let store = var_store.ok_or(ReadError::NullOffset)?;
        let regions = regions.ok_or(ReadError::NullOffset)?;

        // Accumulate deltas directly onto preloaded component values in raw tuple units.
        const ANGLE_SCALE: f32 = 4096.0;
        const SCALE_SCALE: f32 = 1024.0;
        let mut translate_x = transform.translate_x();
        let mut translate_y = transform.translate_y();
        let mut rotation_raw = transform.rotation() * ANGLE_SCALE;
        let mut scale_x_raw = transform.scale_x() * SCALE_SCALE;
        let mut scale_y_raw = transform.scale_y() * SCALE_SCALE;
        let mut skew_x_raw = transform.skew_x() * ANGLE_SCALE;
        let mut skew_y_raw = transform.skew_y() * ANGLE_SCALE;
        let mut center_x = transform.center_x();
        let mut center_y = transform.center_y();

        deltas.clear();
        if flags.contains(VarcFlags::HAVE_TRANSLATE_X) {
            deltas.push(translate_x);
        }
        if flags.contains(VarcFlags::HAVE_TRANSLATE_Y) {
            deltas.push(translate_y);
        }
        if flags.contains(VarcFlags::HAVE_ROTATION) {
            deltas.push(rotation_raw);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_X) {
            deltas.push(scale_x_raw);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_Y) {
            deltas.push(scale_y_raw);
        }
        if flags.contains(VarcFlags::HAVE_SKEW_X) {
            deltas.push(skew_x_raw);
        }
        if flags.contains(VarcFlags::HAVE_SKEW_Y) {
            deltas.push(skew_y_raw);
        }
        if flags.contains(VarcFlags::HAVE_TCENTER_X) {
            deltas.push(center_x);
        }
        if flags.contains(VarcFlags::HAVE_TCENTER_Y) {
            deltas.push(center_y);
        }
        accumulate_tuple_deltas_in_place(
            store,
            regions,
            var_idx,
            coords,
            field_count,
            scalar_cache,
            deltas.as_mut_slice(),
        )?;

        let mut value_iter = deltas.iter().copied();
        if flags.contains(VarcFlags::HAVE_TRANSLATE_X) {
            translate_x = value_iter.next().unwrap_or(translate_x);
        }
        if flags.contains(VarcFlags::HAVE_TRANSLATE_Y) {
            translate_y = value_iter.next().unwrap_or(translate_y);
        }
        if flags.contains(VarcFlags::HAVE_ROTATION) {
            rotation_raw = value_iter.next().unwrap_or(rotation_raw);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_X) {
            scale_x_raw = value_iter.next().unwrap_or(scale_x_raw);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_Y) {
            scale_y_raw = value_iter.next().unwrap_or(scale_y_raw);
        }
        const SKEW_OR_CENTER: VarcFlags = VarcFlags::from_bits_truncate(
            VarcFlags::HAVE_SKEW_X.bits()
                | VarcFlags::HAVE_SKEW_Y.bits()
                | VarcFlags::HAVE_TCENTER_X.bits()
                | VarcFlags::HAVE_TCENTER_Y.bits(),
        );
        if flags.intersects(SKEW_OR_CENTER) {
            if flags.contains(VarcFlags::HAVE_SKEW_X) {
                skew_x_raw = value_iter.next().unwrap_or(skew_x_raw);
            }
            if flags.contains(VarcFlags::HAVE_SKEW_Y) {
                skew_y_raw = value_iter.next().unwrap_or(skew_y_raw);
            }
            if flags.contains(VarcFlags::HAVE_TCENTER_X) {
                center_x = value_iter.next().unwrap_or(center_x);
            }
            if flags.contains(VarcFlags::HAVE_TCENTER_Y) {
                center_y = value_iter.next().unwrap_or(center_y);
            }
        }

        if !flags.contains(VarcFlags::HAVE_SCALE_Y) {
            scale_y_raw = scale_x_raw;
        }
        transform.set_translate_x(translate_x);
        transform.set_translate_y(translate_y);
        transform.set_rotation(rotation_raw / ANGLE_SCALE);
        transform.set_scale_x(scale_x_raw / SCALE_SCALE);
        transform.set_scale_y(scale_y_raw / SCALE_SCALE);
        transform.set_skew_x(skew_x_raw / ANGLE_SCALE);
        transform.set_skew_y(skew_y_raw / ANGLE_SCALE);
        transform.set_center_x(center_x);
        transform.set_center_y(center_y);
        Ok(())
    }

    fn component_condition_met(
        &self,
        component: &VarcComponent<'a>,
        coords: &[F2Dot14],
        var_store: Option<&MultiItemVariationStore<'a>>,
        regions: Option<&SparseVariationRegionList<'a>>,
        scalar_cache: &mut ScalarCache,
        scratch: &mut Scratchpad,
    ) -> Result<bool, DrawError> {
        let Some(condition_index) = component.condition_index() else {
            return Ok(true);
        };
        let Some(condition_list) = self.varc.condition_list() else {
            return Err(DrawError::Read(ReadError::NullOffset));
        };
        let condition_list = condition_list?;
        let condition = condition_list.conditions().get(condition_index as usize)?;
        let store = var_store.ok_or(ReadError::NullOffset)?;
        let regions = regions.ok_or(ReadError::NullOffset)?;
        Self::eval_condition(&condition, coords, store, regions, scalar_cache, scratch)
    }

    fn eval_condition(
        condition: &Condition<'a>,
        coords: &[F2Dot14],
        var_store: &MultiItemVariationStore<'a>,
        regions: &SparseVariationRegionList<'a>,
        scalar_cache: &mut ScalarCache,
        scratch: &mut Scratchpad,
    ) -> Result<bool, DrawError> {
        match condition {
            Condition::Format1AxisRange(condition) => {
                let axis_index = condition.axis_index() as usize;
                let coord = coords.get(axis_index).copied().unwrap_or(F2Dot14::ZERO);
                Ok(coord >= condition.filter_range_min_value()
                    && coord <= condition.filter_range_max_value())
            }
            Condition::Format2VariableValue(condition) => {
                let default_value = condition.default_value() as f32;
                let var_idx = condition.var_index();
                compute_tuple_deltas(
                    var_store,
                    regions,
                    var_idx,
                    coords,
                    1,
                    scalar_cache,
                    &mut scratch.deltas,
                )?;
                let delta = scratch.deltas.first().copied().unwrap_or(0.0);
                Ok(default_value + delta > 0.0)
            }
            Condition::Format3And(condition) => {
                for nested in condition.conditions().iter() {
                    let nested = nested?;
                    if !Self::eval_condition(
                        &nested,
                        coords,
                        var_store,
                        regions,
                        scalar_cache,
                        scratch,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Condition::Format4Or(condition) => {
                for nested in condition.conditions().iter() {
                    let nested = nested?;
                    if Self::eval_condition(
                        &nested,
                        coords,
                        var_store,
                        regions,
                        scalar_cache,
                        scratch,
                    )? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Condition::Format5Negate(condition) => {
                let nested = condition.condition()?;
                Ok(!Self::eval_condition(
                    &nested,
                    coords,
                    var_store,
                    regions,
                    scalar_cache,
                    scratch,
                )?)
            }
        }
    }

    fn scalar_cache_from_store(
        &self,
        store: Option<&MultiItemVariationStore<'a>>,
    ) -> Result<Option<ScalarCache>, DrawError> {
        let Some(store) = store else {
            return Ok(None);
        };
        let region_count = store.region_list()?.region_count() as usize;
        Ok(Some(ScalarCache::new(region_count)))
    }
}

struct ScalarCache {
    values: ScalarCacheVec,
}

impl ScalarCache {
    const INVALID: f32 = 2.0; // Scalars are in [0,1], so 2.0 means "not cached"

    fn new(count: usize) -> Self {
        Self {
            values: ScalarCacheVec::with_len(count, Self::INVALID),
        }
    }

    fn get(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(Self::INVALID)
    }

    fn set(&mut self, index: usize, value: f32) {
        if let Some(slot) = self.values.get_mut(index) {
            *slot = value;
        }
    }
}

fn expand_coords(out: &mut CoordVec, axis_count: usize, coords: &[F2Dot14]) {
    out.resize_and_fill(axis_count, F2Dot14::ZERO);
    for (slot, value) in out.iter_mut().zip(coords.iter().copied()) {
        *slot = value;
    }
}

fn compute_tuple_deltas(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[F2Dot14],
    tuple_len: usize,
    cache: &mut ScalarCache,
    out: &mut DeltaVec,
) -> Result<(), ReadError> {
    out.resize_and_fill(tuple_len, 0.0);
    add_tuple_deltas(
        store,
        regions,
        var_idx,
        coords,
        tuple_len,
        cache,
        out.as_mut_slice(),
    )
}

fn accumulate_tuple_deltas_in_place(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[F2Dot14],
    tuple_len: usize,
    cache: &mut ScalarCache,
    out: &mut [f32],
) -> Result<(), ReadError> {
    add_tuple_deltas(store, regions, var_idx, coords, tuple_len, cache, out)
}

fn add_tuple_deltas(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[F2Dot14],
    tuple_len: usize,
    cache: &mut ScalarCache,
    out: &mut [f32],
) -> Result<(), ReadError> {
    if tuple_len == 0 || var_idx == NO_VARIATION_INDEX {
        return Ok(());
    }
    if out.len() < tuple_len {
        return Err(ReadError::OutOfBounds);
    }
    let out = &mut out[..tuple_len];
    let outer = (var_idx >> 16) as usize;
    let inner = (var_idx & 0xFFFF) as usize;
    let data = store
        .variation_data()
        .get(outer)
        .map_err(|_| ReadError::InvalidCollectionIndex(outer as _))?;
    let region_indices = data.region_indices();
    let mut deltas = data.delta_set(inner)?.fetcher();
    let regions = regions.regions();

    let mut skip = 0;
    for region_index in region_indices.iter() {
        let region_idx = region_index.get() as usize;
        let mut scalar = cache.get(region_idx);
        if scalar >= 2.0 {
            scalar = compute_sparse_region_scalar(&regions.get(region_idx)?, coords);
            cache.set(region_idx, scalar);
        }
        if scalar == 0.0 {
            skip += tuple_len;
            continue;
        }
        if skip != 0 {
            deltas.skip(skip)?;
            skip = 0;
        }
        deltas.add_to_f32_scaled(out, scalar)?;
    }
    Ok(())
}

fn compute_sparse_region_scalar(region: &SparseVariationRegion<'_>, coords: &[F2Dot14]) -> f32 {
    let mut scalar = 1.0f32;
    for axis in region.region_axes() {
        let peak = axis.peak();
        if peak == F2Dot14::ZERO {
            continue;
        }
        let axis_index = axis.axis_index() as usize;
        let coord = coords.get(axis_index).copied().unwrap_or(F2Dot14::ZERO);
        if coord == peak {
            continue;
        }
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
        } else if coord < peak {
            // Use raw bits - scale factors cancel in the ratio
            let numerat = coord.to_bits() - start.to_bits();
            if numerat == 0 {
                return 0.0;
            }
            let denom = peak.to_bits() - start.to_bits();
            scalar *= numerat as f32 / denom as f32;
        } else {
            // Use raw bits - scale factors cancel in the ratio
            let numerat = end.to_bits() - coord.to_bits();
            if numerat == 0 {
                return 0.0;
            }
            let denom = end.to_bits() - peak.to_bits();
            scalar *= numerat as f32 / denom as f32;
        }
    }
    scalar
}

#[inline(always)]
fn scale_matrix(m: Affine, s: f32) -> Affine {
    [m[0], m[1], m[2], m[3], m[4] * s, m[5] * s]
}

const IDENTITY_MATRIX: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[inline(always)]
fn mul_matrix(a: Affine, b: Affine) -> Affine {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

struct TransformPen<'a, P: OutlinePen + ?Sized> {
    pen: &'a mut P,
    matrix: Affine,
}

impl<'a, P: OutlinePen + ?Sized> TransformPen<'a, P> {
    fn new(pen: &'a mut P, matrix: Affine) -> Self {
        Self { pen, matrix }
    }

    #[inline(always)]
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
