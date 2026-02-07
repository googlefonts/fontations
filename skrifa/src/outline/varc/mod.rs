//! Support for rendering variable composite glyphs from the VARC table.

use read_fonts::{
    tables::{
        layout::Condition,
        varc::{
            DecomposedTransform, MultiItemVariationStore, SparseVariationRegionList, Varc,
            VarcComponent, VarcFlags,
        },
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
type CoordRawVec = SmallVec<f32, 64>;
type DeltaVec = SmallVec<f32, 64>;
type ScalarCacheVec = SmallVec<f32, 128>;
type Affine = [f32; 6];
const DEBUG_VARC_TRACE_VAR_IDX: Option<u32> = Some(22_478_849);

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
        let mut font_coords_raw = CoordRawVec::new();
        expand_coords_raw_f32(&mut font_coords_raw, font_coords.as_slice());
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
            font_coords_raw.as_slice(),
            font_coords_raw.as_slice(),
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
            None,
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
        font_coords_raw: &[f32],
        current_coords_raw: &[f32],
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
        trace_var_idx: Option<u32>,
    ) -> Result<(), DrawError> {
        if stack.len() >= GLYF_COMPOSITE_RECURSION_LIMIT {
            return Err(DrawError::RecursionLimitExceeded(glyph_id));
        }
        let glyph = self.varc.glyph(coverage_index as usize)?;
        stack.push(glyph_id);
        let mut current_coords = CoordVec::new();
        expand_coords_from_raw_rounded(&mut current_coords, current_coords_raw);
        let mut component_coords_raw_buffer = CoordRawVec::new();
        let mut component_coords_buffer = CoordVec::new();
        let mut child_scalar_cache: Option<ScalarCache> = None;
        for component in glyph.components() {
            let component = component?;
            if !self.component_condition_met(
                &component,
                current_coords.as_slice(),
                current_coords_raw,
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

            let component_coords_raw = if coords_the_same {
                current_coords_raw
            } else {
                self.component_coords(
                    &component,
                    font_coords_raw,
                    current_coords_raw,
                    var_store,
                    regions,
                    scalar_cache,
                    &mut component_coords_raw_buffer,
                    scratch,
                )?;
                component_coords_raw_buffer.as_slice()
            };

            let mut transform = *component.transform();
            self.apply_transform_variations(
                glyph_id,
                &component,
                current_coords_raw,
                &mut transform,
                var_store,
                regions,
                scalar_cache,
                &mut scratch.deltas,
            )?;
            let component_trace_var_idx = match component.transform_var_index() {
                Some(var_idx) if Some(var_idx) == DEBUG_VARC_TRACE_VAR_IDX => Some(var_idx),
                _ => trace_var_idx,
            };
            let scale = size.linear_scale(self.units_per_em);
            if component_trace_var_idx == DEBUG_VARC_TRACE_VAR_IDX {
                eprintln!(
                    "VARC_LINEAR_SCALE parent_gid={} gid={} var_idx={} ppem={:.16} upem={} scale={:.16} scale_x64={:.16}",
                    glyph_id.to_u32(),
                    component_gid.to_u32(),
                    component_trace_var_idx.unwrap_or_default(),
                    size.ppem().unwrap_or(-1.0) as f64,
                    self.units_per_em,
                    scale as f64,
                    (scale * 64.0) as f64
                );
            }
            let matrix = mul_matrix(
                parent_matrix,
                scale_matrix(normalized_transform_from_raw(transform).matrix(), scale),
            );
            if let Some(var_idx) = component.transform_var_index() {
                if Some(var_idx) == DEBUG_VARC_TRACE_VAR_IDX {
                    eprintln!(
                        "VARC_AFFINE parent_gid={} gid={} var_idx={} xx={:.16} yx={:.16} xy={:.16} yy={:.16} x0={:.16} y0={:.16}",
                        glyph_id.to_u32(),
                        component_gid.to_u32(),
                        var_idx,
                        matrix[0] as f64,
                        matrix[1] as f64,
                        matrix[2] as f64,
                        matrix[3] as f64,
                        matrix[4] as f64,
                        matrix[5] as f64
                    );
                }
            }
            if component_gid != glyph_id {
                if let Some(coverage_index) = coverage.get(component_gid) {
                    if !stack.contains(&component_gid) {
                        // Optimization: if coordinates haven't changed, we can reuse the scalar cache.
                        if coords_the_same {
                            self.draw_glyph(
                                component_gid,
                                coverage_index,
                                font_coords_raw,
                                current_coords_raw,
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
                                component_trace_var_idx,
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
                                font_coords_raw,
                                component_coords_raw,
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
                                component_trace_var_idx,
                            )?;
                        }
                        continue;
                    }
                }
            }
            let component_coords = if coords_the_same {
                current_coords.as_slice()
            } else {
                expand_coords_from_raw_rounded(&mut component_coords_buffer, component_coords_raw);
                component_coords_buffer.as_slice()
            };
            let mut transform_pen = TransformPen::new(
                pen,
                matrix,
                component_trace_var_idx.map(|var_idx| TransformTrace {
                    parent_gid: glyph_id,
                    gid: component_gid,
                    var_idx,
                    seq: 0,
                }),
            );
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
        font_coords_raw: &[f32],
        current_coords_raw: &[f32],
        var_store: Option<&MultiItemVariationStore<'a>>,
        regions: Option<&SparseVariationRegionList<'a>>,
        scalar_cache: &mut ScalarCache,
        coords: &mut CoordRawVec,
        scratch: &mut Scratchpad,
    ) -> Result<(), DrawError> {
        let flags = component.flags();
        if flags.contains(VarcFlags::RESET_UNSPECIFIED_AXES) {
            expand_coords_raw(coords, font_coords_raw.len(), font_coords_raw);
        } else {
            expand_coords_raw(coords, current_coords_raw.len(), current_coords_raw);
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
        for (i, _coord) in current_coords_raw.iter().copied().enumerate() {
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
                current_coords_raw,
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
            let Some(slot) = coords.get_mut(*axis_index as usize) else {
                return Err(DrawError::Read(ReadError::OutOfBounds));
            };
            *slot = value;
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
        parent_gid: GlyphId,
        component: &VarcComponent<'a>,
        coords_raw: &[f32],
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

        debug_trace_coords(parent_gid, component.gid(), var_idx, coords_raw);

        let store = var_store.ok_or(ReadError::NullOffset)?;
        let regions = regions.ok_or(ReadError::NullOffset)?;
        // Match HB rounding behavior: accumulate tuple deltas directly onto
        // preloaded component values instead of summing a standalone delta vector.
        let mut translate_x = transform.translate_x();
        let mut translate_y = transform.translate_y();
        let mut rotation_raw = transform.rotation();
        let mut scale_x_raw = transform.scale_x();
        let mut scale_y_raw = transform.scale_y();
        let mut skew_x_raw = transform.skew_x();
        let mut skew_y_raw = transform.skew_y();
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
            coords_raw,
            field_count,
            scalar_cache,
            deltas.as_mut_slice(),
        )?;

        let rotation_field_index = transform_component_index(flags, VarcFlags::HAVE_ROTATION);
        if let Some(rotation_field_index) = rotation_field_index {
            debug_rotation_region_terms(
                store,
                regions,
                var_idx,
                coords_raw,
                field_count,
                rotation_field_index,
                parent_gid,
                component.gid(),
                scalar_cache,
            )?;
        }

        // Unpack values in flag order.
        let mut value_iter = deltas.iter().copied();
        if flags.contains(VarcFlags::HAVE_TRANSLATE_X) {
            translate_x = value_iter.next().unwrap_or(translate_x);
        }
        if flags.contains(VarcFlags::HAVE_TRANSLATE_Y) {
            translate_y = value_iter.next().unwrap_or(translate_y);
        }
        if flags.contains(VarcFlags::HAVE_ROTATION) {
            let pre_raw = rotation_raw;
            rotation_raw = value_iter.next().unwrap_or(rotation_raw);
            let delta = rotation_raw - pre_raw;
            eprintln!(
                "VARC_ROT parent_gid={} gid={} var_idx={} pre_raw={:.16} pre={:.16} delta_raw={:.16} delta={:.16} post_raw={:.16} post={:.16}",
                parent_gid.to_u32(),
                component.gid().to_u32(),
                var_idx,
                pre_raw as f64,
                (pre_raw / 4096.0) as f64,
                delta as f64,
                (delta / 4096.0) as f64,
                rotation_raw as f64,
                (rotation_raw / 4096.0) as f64
            );
        }
        if flags.contains(VarcFlags::HAVE_SCALE_X) {
            scale_x_raw = value_iter.next().unwrap_or(scale_x_raw);
        }
        if flags.contains(VarcFlags::HAVE_SCALE_Y) {
            scale_y_raw = value_iter.next().unwrap_or(scale_y_raw);
        }
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

        if !flags.contains(VarcFlags::HAVE_SCALE_Y) {
            scale_y_raw = scale_x_raw;
        }
        if Some(var_idx) == DEBUG_VARC_TRACE_VAR_IDX {
            eprintln!(
                "VARC_XFORM parent_gid={} gid={} var_idx={} tx={:.16} ty={:.16} rot_raw={:.16} sx_raw={:.16} sy_raw={:.16} skx_raw={:.16} sky_raw={:.16} cx={:.16} cy={:.16}",
                parent_gid.to_u32(),
                component.gid().to_u32(),
                var_idx,
                translate_x as f64,
                translate_y as f64,
                rotation_raw as f64,
                scale_x_raw as f64,
                scale_y_raw as f64,
                skew_x_raw as f64,
                skew_y_raw as f64,
                center_x as f64,
                center_y as f64
            );
        }
        transform.set_translate_x(translate_x);
        transform.set_translate_y(translate_y);
        transform.set_rotation(rotation_raw);
        transform.set_scale_x(scale_x_raw);
        transform.set_scale_y(scale_y_raw);
        transform.set_skew_x(skew_x_raw);
        transform.set_skew_y(skew_y_raw);
        transform.set_center_x(center_x);
        transform.set_center_y(center_y);
        Ok(())
    }

    fn component_condition_met(
        &self,
        component: &VarcComponent<'a>,
        coords: &[F2Dot14],
        coords_raw: &[f32],
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
        Self::eval_condition(
            &condition,
            coords,
            coords_raw,
            store,
            regions,
            scalar_cache,
            scratch,
        )
    }

    fn eval_condition(
        condition: &Condition<'a>,
        coords: &[F2Dot14],
        coords_raw: &[f32],
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
                    coords_raw,
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
                        coords_raw,
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
                        coords_raw,
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
                    coords_raw,
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

    fn as_mut_slice(&mut self) -> &mut [f32] {
        self.values.as_mut_slice()
    }
}

fn expand_coords(out: &mut CoordVec, axis_count: usize, coords: &[F2Dot14]) {
    out.resize_and_fill(axis_count, F2Dot14::ZERO);
    for (slot, value) in out.iter_mut().zip(coords.iter().copied()) {
        *slot = value;
    }
}

fn expand_coords_raw(out: &mut CoordRawVec, axis_count: usize, coords: &[f32]) {
    out.resize_and_fill(axis_count, 0.0);
    for (slot, value) in out.iter_mut().zip(coords.iter().copied()) {
        *slot = value;
    }
}

fn expand_coords_raw_f32(out: &mut CoordRawVec, coords: &[F2Dot14]) {
    out.resize_and_fill(coords.len(), 0.0);
    for (slot, value) in out.iter_mut().zip(coords.iter().copied()) {
        *slot = value.to_bits() as f32;
    }
}

#[inline(always)]
fn round_raw_f2dot14(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    let rounded = if value >= 0.0 {
        value + 0.5
    } else {
        value - 0.5
    };
    if rounded < i16::MIN as f32 {
        i16::MIN
    } else if rounded > i16::MAX as f32 {
        i16::MAX
    } else {
        rounded as i16
    }
}

fn transform_component_index(flags: VarcFlags, target: VarcFlags) -> Option<usize> {
    let mut index = 0usize;
    for flag in [
        VarcFlags::HAVE_TRANSLATE_X,
        VarcFlags::HAVE_TRANSLATE_Y,
        VarcFlags::HAVE_ROTATION,
        VarcFlags::HAVE_SCALE_X,
        VarcFlags::HAVE_SCALE_Y,
        VarcFlags::HAVE_SKEW_X,
        VarcFlags::HAVE_SKEW_Y,
        VarcFlags::HAVE_TCENTER_X,
        VarcFlags::HAVE_TCENTER_Y,
    ] {
        if flags.contains(flag) {
            if flag == target {
                return Some(index);
            }
            index += 1;
        }
    }
    None
}

fn debug_trace_coords(parent_gid: GlyphId, gid: GlyphId, var_idx: u32, coords_raw: &[f32]) {
    if Some(var_idx) != DEBUG_VARC_TRACE_VAR_IDX {
        return;
    }
    eprint!(
        "VARC_COORDS parent_gid={} gid={} var_idx={} len={}",
        parent_gid.to_u32(),
        gid.to_u32(),
        var_idx,
        coords_raw.len()
    );
    for (axis, raw) in coords_raw.iter().copied().enumerate() {
        let rounded = round_raw_f2dot14(raw) as i32;
        eprint!(" {}:{:.16}/{}", axis, raw as f64, rounded);
    }
    eprintln!();
}

fn expand_coords_from_raw_rounded(out: &mut CoordVec, coords: &[f32]) {
    out.resize_and_fill(coords.len(), F2Dot14::ZERO);
    for (slot, value) in out.iter_mut().zip(coords.iter().copied()) {
        *slot = F2Dot14::from_bits(round_raw_f2dot14(value));
    }
}

fn compute_tuple_deltas(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[f32],
    tuple_len: usize,
    cache: &mut ScalarCache,
    out: &mut DeltaVec,
) -> Result<(), ReadError> {
    out.resize_and_fill(tuple_len, 0.0);
    store.add_tuple_deltas_raw_f32(
        regions,
        var_idx,
        coords,
        tuple_len,
        out.as_mut_slice(),
        Some(cache.as_mut_slice()),
    )
}

fn accumulate_tuple_deltas_in_place(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[f32],
    tuple_len: usize,
    cache: &mut ScalarCache,
    out: &mut [f32],
) -> Result<(), ReadError> {
    store.add_tuple_deltas_raw_f32(
        regions,
        var_idx,
        coords,
        tuple_len,
        out,
        Some(cache.as_mut_slice()),
    )
}

fn debug_rotation_region_terms(
    store: &MultiItemVariationStore,
    regions: &SparseVariationRegionList,
    var_idx: u32,
    coords: &[f32],
    tuple_len: usize,
    rotation_index: usize,
    parent_gid: GlyphId,
    gid: GlyphId,
    cache: &mut ScalarCache,
) -> Result<(), ReadError> {
    if tuple_len == 0 || rotation_index >= tuple_len {
        return Ok(());
    }

    let outer = (var_idx >> 16) as usize;
    let inner = (var_idx & 0xFFFF) as usize;
    let data = store
        .variation_data()
        .get(outer)
        .map_err(|_| ReadError::InvalidCollectionIndex(outer as _))?;
    let region_indices = data.region_indices();
    let all_regions = regions.regions();
    let mut deltas = data.delta_set(inner)?.fetcher();
    let mut skip = 0usize;
    let mut tmp = DeltaVec::with_len(tuple_len, 0.0);
    let mut running_f32 = 0.0f32;
    let mut running_f64 = 0.0f64;

    for (region_order, region_index) in region_indices.iter().enumerate() {
        let region_idx = region_index.get() as usize;
        let scalar = if let Some(slot) = cache.values.get_mut(region_idx) {
            if *slot <= 1.0 {
                *slot
            } else {
                let computed = all_regions.get(region_idx)?.compute_scalar_raw_f32(coords);
                *slot = computed;
                computed
            }
        } else {
            all_regions.get(region_idx)?.compute_scalar_raw_f32(coords)
        };

        if scalar == 0.0 {
            skip += tuple_len;
            eprintln!(
                "VARC_ROT_REGION parent_gid={} gid={} var_idx={} region_order={} region_idx={} scalar={:.16} raw={:.16} contrib={:.16} running_f32={:.16} running_f64={:.16}",
                parent_gid.to_u32(),
                gid.to_u32(),
                var_idx,
                region_order,
                region_idx,
                0.0f64,
                0.0f64,
                0.0f64,
                running_f32 as f64,
                running_f64
            );
            continue;
        }

        if skip != 0 {
            deltas.skip(skip)?;
            skip = 0;
        }
        tmp.as_mut_slice().fill(0.0);
        deltas.add_to_f32_scaled(tmp.as_mut_slice(), 1.0)?;
        let raw = tmp[rotation_index];
        let contrib = raw * scalar;
        running_f32 += contrib;
        running_f64 += (raw as f64) * (scalar as f64);
        eprintln!(
            "VARC_ROT_REGION parent_gid={} gid={} var_idx={} region_order={} region_idx={} scalar={:.16} raw={:.16} contrib={:.16} running_f32={:.16} running_f64={:.16}",
            parent_gid.to_u32(),
            gid.to_u32(),
            var_idx,
            region_order,
            region_idx,
            scalar as f64,
            raw as f64,
            contrib as f64,
            running_f32 as f64,
            running_f64
        );
    }
    Ok(())
}

#[inline(always)]
fn normalized_transform_from_raw(mut transform: DecomposedTransform) -> DecomposedTransform {
    transform.set_rotation(transform.rotation() / 4096.0);
    transform.set_scale_x(transform.scale_x() / 1024.0);
    transform.set_scale_y(transform.scale_y() / 1024.0);
    transform.set_skew_x(transform.skew_x() / 4096.0);
    transform.set_skew_y(transform.skew_y() / 4096.0);
    transform
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
    trace: Option<TransformTrace>,
}

#[derive(Copy, Clone)]
struct TransformTrace {
    parent_gid: GlyphId,
    gid: GlyphId,
    var_idx: u32,
    seq: u32,
}

impl<'a, P: OutlinePen + ?Sized> TransformPen<'a, P> {
    fn new(pen: &'a mut P, matrix: Affine, trace: Option<TransformTrace>) -> Self {
        Self { pen, matrix, trace }
    }

    #[inline(always)]
    fn transform(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.matrix;
        (a * x + c * y + e, b * x + d * y + f)
    }

    fn next_seq(&mut self) -> Option<u32> {
        let trace = self.trace.as_mut()?;
        let seq = trace.seq;
        trace.seq = trace.seq.saturating_add(1);
        Some(seq)
    }

    fn trace_point(&self, phase: &str, seq: u32, op: &str, x: f32, y: f32) {
        let Some(trace) = self.trace else {
            return;
        };
        eprintln!(
            "VARC_CMD phase={} parent_gid={} gid={} var_idx={} seq={} op={} x_raw_26_6={:.16} y_raw_26_6={:.16} x_norm={:.16} y_norm={:.16} x_cmp={:.16} y_cmp={:.16}",
            phase,
            trace.parent_gid.to_u32(),
            trace.gid.to_u32(),
            trace.var_idx,
            seq,
            op,
            (x * 64.0) as f64,
            (y * 64.0) as f64,
            x as f64,
            y as f64,
            x as f64,
            y as f64
        );
    }

    fn trace_quad(&self, phase: &str, seq: u32, op: &str, cx0: f32, cy0: f32, x: f32, y: f32) {
        let Some(trace) = self.trace else {
            return;
        };
        eprintln!(
            "VARC_CMD phase={} parent_gid={} gid={} var_idx={} seq={} op={} cx0_raw_26_6={:.16} cy0_raw_26_6={:.16} x_raw_26_6={:.16} y_raw_26_6={:.16} cx0_norm={:.16} cy0_norm={:.16} x_norm={:.16} y_norm={:.16} cx0_cmp={:.16} cy0_cmp={:.16} x_cmp={:.16} y_cmp={:.16}",
            phase,
            trace.parent_gid.to_u32(),
            trace.gid.to_u32(),
            trace.var_idx,
            seq,
            op,
            (cx0 * 64.0) as f64,
            (cy0 * 64.0) as f64,
            (x * 64.0) as f64,
            (y * 64.0) as f64,
            cx0 as f64,
            cy0 as f64,
            x as f64,
            y as f64,
            cx0 as f64,
            cy0 as f64,
            x as f64,
            y as f64
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn trace_cubic(
        &self,
        phase: &str,
        seq: u32,
        op: &str,
        cx0: f32,
        cy0: f32,
        cx1: f32,
        cy1: f32,
        x: f32,
        y: f32,
    ) {
        let Some(trace) = self.trace else {
            return;
        };
        eprintln!(
            "VARC_CMD phase={} parent_gid={} gid={} var_idx={} seq={} op={} cx0_raw_26_6={:.16} cy0_raw_26_6={:.16} cx1_raw_26_6={:.16} cy1_raw_26_6={:.16} x_raw_26_6={:.16} y_raw_26_6={:.16} cx0_norm={:.16} cy0_norm={:.16} cx1_norm={:.16} cy1_norm={:.16} x_norm={:.16} y_norm={:.16} cx0_cmp={:.16} cy0_cmp={:.16} cx1_cmp={:.16} cy1_cmp={:.16} x_cmp={:.16} y_cmp={:.16}",
            phase,
            trace.parent_gid.to_u32(),
            trace.gid.to_u32(),
            trace.var_idx,
            seq,
            op,
            (cx0 * 64.0) as f64,
            (cy0 * 64.0) as f64,
            (cx1 * 64.0) as f64,
            (cy1 * 64.0) as f64,
            (x * 64.0) as f64,
            (y * 64.0) as f64,
            cx0 as f64,
            cy0 as f64,
            cx1 as f64,
            cy1 as f64,
            x as f64,
            y as f64,
            cx0 as f64,
            cy0 as f64,
            cx1 as f64,
            cy1 as f64,
            x as f64,
            y as f64
        );
    }

    fn trace_close(&self, phase: &str, seq: u32, op: &str) {
        let Some(trace) = self.trace else {
            return;
        };
        eprintln!(
            "VARC_CMD phase={} parent_gid={} gid={} var_idx={} seq={} op={}",
            phase,
            trace.parent_gid.to_u32(),
            trace.gid.to_u32(),
            trace.var_idx,
            seq,
            op
        );
    }
}

impl<P: OutlinePen + ?Sized> OutlinePen for TransformPen<'_, P> {
    fn move_to(&mut self, x: f32, y: f32) {
        let seq = self.next_seq();
        if let Some(seq) = seq {
            self.trace_point("pre", seq, "M", x, y);
        }
        let (tx, ty) = self.transform(x, y);
        if let Some(seq) = seq {
            self.trace_point("post", seq, "M", tx, ty);
        }
        self.pen.move_to(tx, ty);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let seq = self.next_seq();
        if let Some(seq) = seq {
            self.trace_point("pre", seq, "L", x, y);
        }
        let (tx, ty) = self.transform(x, y);
        if let Some(seq) = seq {
            self.trace_point("post", seq, "L", tx, ty);
        }
        self.pen.line_to(tx, ty);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let seq = self.next_seq();
        if let Some(seq) = seq {
            self.trace_quad("pre", seq, "Q", cx0, cy0, x, y);
        }
        let (tcx0, tcy0) = self.transform(cx0, cy0);
        let (tx, ty) = self.transform(x, y);
        if let Some(seq) = seq {
            self.trace_quad("post", seq, "Q", tcx0, tcy0, tx, ty);
        }
        self.pen.quad_to(tcx0, tcy0, tx, ty);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let seq = self.next_seq();
        if let Some(seq) = seq {
            self.trace_cubic("pre", seq, "C", cx0, cy0, cx1, cy1, x, y);
        }
        let (tcx0, tcy0) = self.transform(cx0, cy0);
        let (tcx1, tcy1) = self.transform(cx1, cy1);
        let (tx, ty) = self.transform(x, y);
        if let Some(seq) = seq {
            self.trace_cubic("post", seq, "C", tcx0, tcy0, tcx1, tcy1, tx, ty);
        }
        self.pen.curve_to(tcx0, tcy0, tcx1, tcy1, tx, ty);
    }

    fn close(&mut self) {
        let seq = self.next_seq();
        if let Some(seq) = seq {
            self.trace_close("pre", seq, "Z");
            self.trace_close("post", seq, "Z");
        }
        self.pen.close();
    }
}
