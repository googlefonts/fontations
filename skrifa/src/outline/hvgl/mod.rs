//! Support for scaling
//! [hvgl](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6hvgl.html)
//! outlines.
//!
//! The implementation was heavily based on
//! [HarfBuzz's](https://github.com/harfbuzz/harfbuzz/blob/3c81945/src/hb-aat-var-hvgl-table.cc#L392-L573).

mod memory;
mod outline;
mod transform;

pub use outline::Outline;

use raw::{
    tables::hvgl::{
        CompositePart, CoordBlendType, Hvgl, LittleEndian, Part, PartArray, ShapePart,
        TranslationDelta,
    },
    types::{F2Dot14, GlyphId, Point},
    FontRef, ReadError, TableProvider,
};

use crate::outline::{
    hvgl::{memory::HvglOutlineMemory, transform::Transform},
    metrics::GlyphHMetrics,
    DrawError, OutlinePen,
};

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::CoreFloat;

use bytemuck::{AnyBitPattern, NoUninit};

#[derive(Clone)]
pub(crate) struct Outlines<'a> {
    pub(crate) font: FontRef<'a>,
    pub(crate) glyph_metrics: GlyphHMetrics<'a>,
    hvgl: Hvgl<'a>,
    //hvpm: Option<Hvpm<'a>>,
    units_per_em: u16,
}

const MAX_RECURSION_DEPTH: usize = 32;

/// A single curve segment in a contour. Note that for curve-type segments,
/// `on_curve_x` actually stores the "parallel factor" (distance factor between
/// the previous and next off-curve point).
#[derive(Clone, Copy, Default, AnyBitPattern, NoUninit)]
#[repr(C)]
struct Segment {
    on_curve_x: f64,
    on_curve_y: f64,
    off_curve_x: f64,
    off_curve_y: f64,
}

impl Segment {
    fn project_on_curve_to_tangent(mut self, offcurve1: &Self, offcurve2: &Self) -> Self {
        let x = &mut self.on_curve_x;
        let y = &mut self.on_curve_y;

        let x1 = offcurve1.off_curve_x;
        let y1 = offcurve1.off_curve_y;
        let x2 = offcurve2.off_curve_x;
        let y2 = offcurve2.off_curve_y;

        let dx = x2 - x1;
        let dy = y2 - y1;

        let l2 = (dx * dx) + (dy * dy);
        let mut t = if l2 != 0.0 {
            ((dx * (*x - x1)) + (dy * (*y - y1))) / l2
        } else {
            0.0
        };
        t = t.clamp(0.0, 1.0);

        *x = x1 + (dx * t);
        *y = y1 + (dy * t);

        self
    }
}

impl<'a> From<&'a [LittleEndian<f64>]> for Segment {
    #[inline(always)]
    fn from(value: &'a [LittleEndian<f64>]) -> Self {
        Self {
            on_curve_x: value[0].get(),
            on_curve_y: value[1].get(),
            off_curve_x: value[2].get(),
            off_curve_y: value[3].get(),
        }
    }
}

/// Turn a [`Segment`], which can mean different things depending on its blend
/// type, into an absolutely-positioned quadratic segment with one on-curve and
/// one off-curve point.
fn resolve_blend_type(
    prev_segment: &Segment,
    cur_segment: &Segment,
    next_segment: &mut Segment,
    blend_type: CoordBlendType,
) -> Result<Segment, ReadError> {
    Ok(match blend_type {
        CoordBlendType::Curve => {
            // not actually the on-curve X, but the parallel factor for curve-type segments
            let t = cur_segment.on_curve_x.clamp(0.0, 1.0);

            let x = prev_segment.off_curve_x
                + ((cur_segment.off_curve_x - prev_segment.off_curve_x) * t);
            let y = prev_segment.off_curve_y
                + ((cur_segment.off_curve_y - prev_segment.off_curve_y) * t);

            Segment {
                on_curve_x: x,
                on_curve_y: y,
                ..*cur_segment
            }
        }
        CoordBlendType::Corner | CoordBlendType::SecondTangent => *cur_segment,
        CoordBlendType::IsolatedTangent => {
            cur_segment.project_on_curve_to_tangent(prev_segment, cur_segment)
        }
        CoordBlendType::FirstTangent => {
            let resolved_segment =
                cur_segment.project_on_curve_to_tangent(prev_segment, &next_segment);
            // TODO: Mutating the next segment seems suspicious, but it's what
            // HarfBuzz does. This blend type seems unused, so I can't test if
            // it's correct.
            *next_segment = next_segment.project_on_curve_to_tangent(prev_segment, &next_segment);
            resolved_segment
        }
        // ReadError::InvalidFormat requires us to return the
        // invalid format itself, which we lose during parsing
        _ => return Err(ReadError::MalformedData("Unknown blend type")),
    })
}

impl<'a> Outlines<'a> {
    pub fn new(font: &FontRef<'a>) -> Option<Self> {
        let glyph_metrics = GlyphHMetrics::new(font)?;
        let hvgl = font.hvgl().ok()?;
        let units_per_em = font.head().ok()?.units_per_em();
        Some(Self {
            font: font.clone(),
            glyph_metrics,
            hvgl,
            units_per_em,
        })
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn glyph_count(&self) -> u32 {
        self.hvgl.num_glyphs()
    }

    fn compute_scale(&self, ppem: Option<f32>) -> f64 {
        match ppem {
            Some(ppem) => ppem as f64 / self.units_per_em as f64,
            None => 1.0,
        }
    }

    /// Fetch the part at the given part index. This does not necessarily have
    /// to be a glyph ID.
    #[inline]
    fn part_at(&self, i: u32) -> Result<Option<Part<'a>>, ReadError> {
        // TODO: remap with hvpm table if present
        let part_index = self.hvgl.part_index()?;
        let parts: PartArray<'a> = part_index.parts();
        match parts.get(i as usize) {
            Ok(part) => Ok(Some(part)),
            Err(ReadError::InvalidCollectionIndex(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Fetch a part for a given glyph ID. If the passed glyph ID is outside the
    /// range of top-level parts, returns [`None`].
    fn part_for_glyph_id(&self, id: GlyphId) -> Result<Option<Part<'a>>, ReadError> {
        if id.to_u32() >= self.hvgl.num_glyphs() {
            return Ok(None);
        }

        self.part_at(id.to_u32())
    }

    pub fn draw(
        &self,
        outline: &Outline<'_>,
        buf: &'a mut [u8],
        ppem: Option<f32>,
        coords: &[F2Dot14],
        pen: &mut impl OutlinePen,
    ) -> Result<(), DrawError> {
        let scale = self.compute_scale(ppem);
        let part = &outline.part;
        let memory = HvglOutlineMemory::new(outline, buf).ok_or(DrawError::InsufficientMemory)?;
        {
            let (provided_coords, remaining_coords) = memory
                .coords
                .split_at_mut(coords.len().min(memory.coords.len()));
            for (dst, src) in provided_coords.iter_mut().zip(coords.iter()) {
                *dst = src.to_f32();
            }
            remaining_coords.fill(0.0);
        }
        memory.transforms.fill(Transform::default());

        self.draw_part(
            part,
            scale,
            memory.coords,
            memory.transforms,
            memory.segments,
            pen,
        )
    }

    pub fn outline(&self, glyph_id: GlyphId) -> Result<Outline<'a>, DrawError> {
        let part = self
            .part_for_glyph_id(glyph_id)?
            .ok_or(DrawError::GlyphNotFound(glyph_id))?;
        let mut outline = Outline {
            glyph_id,
            part: part.clone(),
            max_num_segments: 0,
        };
        self.outline_rec(&mut outline, &part, 0)?;
        Ok(outline)
    }

    /// Crawl a glyph part to determine the maximum segment count--this lets us
    /// know how much temp memory to allocate. All the offset-chasing is quite
    /// slow; it would sure be nice if this info was included in the composite
    /// part header.
    fn outline_rec(
        &self,
        outline: &mut Outline<'_>,
        part: &Part<'_>,
        recursion_depth: usize,
    ) -> Result<(), DrawError> {
        if recursion_depth >= MAX_RECURSION_DEPTH {
            return Err(DrawError::RecursionLimitExceeded(outline.glyph_id));
        }
        match part {
            Part::Shape(shape) => {
                outline.max_num_segments = outline.max_num_segments.max(shape.num_segments());
            }
            Part::Composite(composite) => {
                for part in composite.subparts()? {
                    let part = self
                        .part_at(part.part_table_index())?
                        .ok_or(DrawError::GlyphNotFound(part.part_table_index().into()))?;
                    self.outline_rec(outline, &part, recursion_depth + 1)?;
                }
            }
        }
        Ok(())
    }

    /// Draw a shape part (a "leaf node" that contains the actual contours).
    ///
    /// See <https://github.com/harfbuzz/harfbuzz/blob/3c81945/src/hb-aat-var-hvgl-table.cc#L90-L347>.
    fn draw_shape_part(
        &self,
        shape: &'a ShapePart<'a>,
        scale: f64,
        coords: &mut [f32],
        transforms: &mut [Transform],
        segment_memory: &mut [Segment],
        pen: &mut impl OutlinePen,
    ) -> Result<(), DrawError> {
        let total_num_segments = shape.num_segments() as usize;
        if total_num_segments == 0 {
            return Ok(());
        }

        let transform = &transforms[0];
        let num_axes = shape.num_axes() as usize;
        let points = shape.master_coordinate_vector();
        let mut blend_types = shape.blend_types();
        let deltas = shape.delta_coordinate_matrix();
        let shape_coords = coords.get(..num_axes).ok_or(ReadError::OutOfBounds)?;

        // We're guaranteed (because we preparsed the glyph) that segment_memory
        // is big enough. But I haven't benchmarked if a `return` is faster than
        // a panic yet.
        let mut all_segments = segment_memory
            .get_mut(..total_num_segments)
            .ok_or(ReadError::OutOfBounds)?;
        for (segment, segment_points) in all_segments.iter_mut().zip(points.chunks_exact(4)) {
            *segment = Segment::from(segment_points);
        }

        // Apply the coordinate deltas. There's a reason they're stored in the
        // data an axis at a time instead of a segment at a time: summing a
        // bunch of values (the deltas) into a small number of accumulators (the
        // segment points) is bottlenecked on waiting for the previous iteration
        // to finish. Summing things one axis at a time can be parallelized and
        // pipelined, increasing performance.
        for (coord, axis_deltas) in shape_coords
            .iter()
            .zip(deltas.chunks_exact(total_num_segments * 8))
        {
            if *coord == 0.0 {
                continue;
            }
            let axis_deltas = if *coord > 0.0 {
                &axis_deltas[total_num_segments * 4..]
            } else {
                &axis_deltas[..total_num_segments * 4]
            };
            let scalar = coord.abs() as f64;

            for (segment, delta) in all_segments.iter_mut().zip(axis_deltas.chunks_exact(4)) {
                // This is autovectorized quite well, but we don't get FMA. I'm
                // not sure if it's desirable, considering that only some
                // targets support it, and it would result in slightly different
                // rounding.
                segment.on_curve_x += delta[0].get() * scalar;
                segment.on_curve_y += delta[1].get() * scalar;
                segment.off_curve_x += delta[2].get() * scalar;
                segment.off_curve_y += delta[3].get() * scalar;
            }
        }

        for segment_count in shape.path_sizes() {
            let segment_count = segment_count.get() as usize;
            if segment_count < 2 {
                // This comment serves as a hint to the person reading
                // it that this branch is unlikely to be taken
                continue;
            }
            let (segments, tail) = all_segments
                .split_at_mut_checked(segment_count)
                .ok_or(ReadError::OutOfBounds)?;
            all_segments = tail;
            let (subpath_blend_types, tail) = blend_types
                .split_at_checked(segment_count)
                .ok_or(ReadError::OutOfBounds)?;
            blend_types = tail;

            let first_segment = *segments.first().unwrap();
            let last_segment = *segments.last().unwrap();
            let mut prev_segment = last_segment;
            let mut cur_segment = first_segment;

            let mut prev_blended_segment: Option<Segment> = None;
            let mut first_point = Point::<f64>::default();

            for i in 0..segment_count {
                let mut next_segment = if i == segment_count - 1 {
                    first_segment
                } else if i == segment_count - 2 {
                    last_segment
                } else {
                    segments[i + 1]
                };

                let blend_type = subpath_blend_types[i].get();

                let blended_segment =
                    resolve_blend_type(&prev_segment, &cur_segment, &mut next_segment, blend_type)?;
                if let Some(prev) = prev_blended_segment {
                    let p0 = transform
                        .transform_point(Point::new(prev.off_curve_x, prev.off_curve_y))
                        * scale;
                    let p = transform.transform_point(Point::new(
                        blended_segment.on_curve_x,
                        blended_segment.on_curve_y,
                    )) * scale;
                    pen.quad_to(p0.x as f32, p0.y as f32, p.x as f32, p.y as f32);
                } else {
                    let p = transform.transform_point(Point::new(
                        blended_segment.on_curve_x,
                        blended_segment.on_curve_y,
                    )) * scale;
                    pen.move_to(p.x as f32, p.y as f32);
                    first_point =
                        Point::new(blended_segment.on_curve_x, blended_segment.on_curve_y);
                }

                prev_blended_segment = Some(blended_segment);
                prev_segment = cur_segment;
                cur_segment = next_segment;
            }

            let prev = prev_blended_segment.unwrap();
            let p0 =
                transform.transform_point(Point::new(prev.off_curve_x, prev.off_curve_y)) * scale;
            let p = transform.transform_point(first_point) * scale;
            pen.quad_to(p0.x as f32, p0.y as f32, p.x as f32, p.y as f32);
            pen.close();
        }

        Ok(())
    }

    /// Apply a composite part's delta coords and transforms to its subparts, and then draw them in turn.
    ///
    /// See <https://github.com/harfbuzz/harfbuzz/blob/3c81945/src/hb-aat-var-hvgl-table.cc#L576-L613>.
    fn draw_composite_part(
        &self,
        composite: &'a CompositePart<'a>,
        scale: f64,
        coords: &mut [f32],
        transforms: &mut [Transform],
        segment_memory: &mut [Segment],
        pen: &mut impl OutlinePen,
    ) -> Result<(), DrawError> {
        let num_total_axes = composite.num_total_axes() as usize;
        let coords_len = coords.len();
        let coords = &mut coords[..num_total_axes.min(coords_len)];

        let (my_coords, child_coords) = coords
            .split_at_mut_checked(composite.num_direct_axes().into())
            .ok_or(ReadError::OutOfBounds)?;

        composite_apply_to_coords(&composite, child_coords, my_coords)?;

        let num_total_subparts = composite.num_total_subparts() as usize;
        let transforms_len = transforms.len();
        let transforms = &mut transforms[0..num_total_subparts.min(transforms_len)];

        let (transforms_head, transforms_tail) =
            transforms.split_first_mut().ok_or(ReadError::OutOfBounds)?;

        composite_apply_to_transforms(&composite, transforms_tail, my_coords)?;

        for subpart in composite.subparts()? {
            // Fetching the subpart early is important for performance. It's a
            // probably-random memory access, so it has high latency. The
            // compiler can't reorder the code for us because of the
            // error-handling branches.
            let subpart_part = self
                .part_at(subpart.part_table_index())?
                .ok_or_else(|| DrawError::GlyphNotFound(subpart.part_table_index().into()))?;
            let subpart_coords = child_coords
                .get_mut(subpart.tree_axis_index() as usize..)
                .ok_or(ReadError::OutOfBounds)?;

            let this_transform = transforms_tail
                .get_mut(subpart.tree_part_index() as usize)
                .ok_or(ReadError::OutOfBounds)?;
            if this_transform.is_translation() {
                let Transform { dx, dy, .. } = *this_transform;
                *this_transform = *transforms_head;
                *this_transform = this_transform.translate(dx, dy);
            } else {
                *this_transform = *transforms_head * *this_transform;
            }

            let subpart_transforms = transforms_tail
                .get_mut(subpart.tree_part_index() as usize..)
                .ok_or(ReadError::OutOfBounds)?;
            self.draw_part(
                &subpart_part,
                scale,
                subpart_coords,
                subpart_transforms,
                segment_memory,
                pen,
            )?;
        }

        Ok(())
    }

    #[inline(always)]
    fn draw_part(
        &self,
        part: &Part<'a>,
        scale: f64,
        coords: &mut [f32],
        transforms: &mut [Transform],
        segment_memory: &mut [Segment],
        pen: &mut impl OutlinePen,
    ) -> Result<(), DrawError> {
        match part {
            Part::Shape(shape) => {
                self.draw_shape_part(&shape, scale, coords, transforms, segment_memory, pen)
            }
            Part::Composite(composite) => {
                self.draw_composite_part(&composite, scale, coords, transforms, segment_memory, pen)
            }
        }
    }
}

/// Apply a composite part's deltas to its subparts' deltas. They're stored in a sparse format.
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/3c81945/src/hb-aat-var-hvgl-table.cc#L350-L389>.
fn composite_apply_to_coords<'a>(
    part: &'a CompositePart<'a>,
    out_coords: &mut [f32],
    coords: &[f32],
) -> Result<(), ReadError> {
    let ecs = part.extremum_column_starts()?;
    let extremum_row_indices = part.extremum_row_indices()?;
    let extremum_axis_value_deltas = part.extremum_axis_value_deltas()?;

    for (row_idx, delta) in part
        .master_row_indices()?
        .iter()
        .zip(part.master_axis_value_deltas()?)
    {
        *out_coords
            .get_mut(row_idx.get() as usize)
            .ok_or(ReadError::OutOfBounds)? += delta.get();
    }

    for (axis_idx, coord) in coords.iter().copied().enumerate() {
        if coord == 0.0 {
            continue;
        }
        let pos = (coord > 0.0) as usize;
        let column_idx = (axis_idx * 2) + pos;
        let scalar = coord.abs();

        let sparse_row_start = ecs.get(column_idx).ok_or(ReadError::OutOfBounds)?.get() as usize;
        let sparse_row_end = ecs
            .get(column_idx.saturating_add(1))
            .ok_or(ReadError::OutOfBounds)?
            .get() as usize;

        for row_idx in sparse_row_start..sparse_row_end.min(extremum_axis_value_deltas.len()) {
            let row = extremum_row_indices
                .get(row_idx)
                .ok_or(ReadError::OutOfBounds)?
                .get();
            let delta = extremum_axis_value_deltas
                .get(row_idx)
                .ok_or(ReadError::OutOfBounds)?
                .get();
            *out_coords
                .get_mut(row as usize)
                .ok_or(ReadError::OutOfBounds)? += delta * scalar;
        }
    }

    Ok(())
}

/// Apply a composite part's transforms to its subparts. They're stored in a sparse format.
///
/// See <https://github.com/harfbuzz/harfbuzz/blob/3c81945/src/hb-aat-var-hvgl-table.cc#L392-L573>.
fn composite_apply_to_transforms<'a>(
    part: &'a CompositePart<'a>,
    transforms: &mut [Transform],
    coords: &[f32],
) -> Result<(), ReadError> {
    let master_translation_deltas = part.master_translation_deltas()?;
    let mut extremum_translation_deltas = part.extremum_translation_deltas()?;
    let mut extremum_translation_indices = part.extremum_translation_indices()?;
    let master_translation_indices = part.master_translation_indices()?;

    let master_rotation_deltas = part.master_rotation_deltas()?;
    let mut extremum_rotation_deltas = part.extremum_rotation_deltas()?;
    let mut extremum_rotation_indices = part.extremum_rotation_indices()?;
    let master_rotation_indices = part.master_rotation_indices()?;

    if part.num_extremum_rotations() == 0 {
        // If there are no rotations, we can skip most of the complex logic.
        for (idx, delta) in extremum_translation_indices
            .iter()
            .zip(extremum_translation_deltas.iter())
        {
            let row = idx.row();
            let column = idx.column();
            let delta = delta.get();

            let axis_idx = column / 2;
            let coord = *coords
                .get(axis_idx as usize)
                .ok_or(ReadError::OutOfBounds)?;
            if coord == 0.0 {
                continue;
            }
            let pos = (column & 1) == 1;
            if pos != (coord > 0.0) {
                continue;
            }

            let Some(dst) = transforms.get_mut(row as usize) else {
                break;
            };

            let scalar = coord.abs() as f64;
            *dst = dst.pre_translate(delta.x as f64 * scalar, delta.y as f64 * scalar);
        }
    } else {
        loop {
            let mut row = transforms.len();
            if let Some(tr_index) = extremum_translation_indices.get(0) {
                row = row.min(tr_index.row() as usize);
            }
            if let Some(rot_index) = extremum_rotation_indices.get(0) {
                row = row.min(rot_index.row() as usize);
            }
            if row == transforms.len() {
                break;
            }

            let mut transform = Transform::default();
            let mut is_translate_only = true;

            loop {
                let row_translation = extremum_translation_indices
                    .get(0)
                    .filter(|i| i.row() as usize == row);
                let row_rotation = extremum_rotation_indices
                    .get(0)
                    .filter(|i| i.row() as usize == row);

                let mut column = 2 * part.num_direct_axes() as usize;
                if let Some(tr_index) = row_translation {
                    column = column.min(tr_index.column() as usize);
                }
                if let Some(rot_index) = row_rotation {
                    column = column.min(rot_index.column() as usize);
                }
                if column == 2 * part.num_direct_axes() as usize {
                    break;
                }

                let mut extremum_translation_delta = TranslationDelta::default();
                let mut extremum_rotation_delta = 0.0;

                if row_translation.is_some_and(|tr_index| tr_index.column() as usize == column) {
                    extremum_translation_delta = extremum_translation_deltas
                        .get(0)
                        .ok_or(ReadError::OutOfBounds)?
                        .get();
                    extremum_translation_indices = &extremum_translation_indices[1..];
                    extremum_translation_deltas = &extremum_translation_deltas[1..];
                }

                if row_rotation.is_some_and(|rot_index| rot_index.column() as usize == column) {
                    extremum_rotation_delta = extremum_rotation_deltas
                        .get(0)
                        .ok_or(ReadError::OutOfBounds)?
                        .get();
                    extremum_rotation_indices = &extremum_rotation_indices[1..];
                    extremum_rotation_deltas = &extremum_rotation_deltas[1..];
                }

                let axis_idx = column / 2;
                let coord = *coords.get(axis_idx).ok_or(ReadError::OutOfBounds)?;
                if coord == 0.0 {
                    continue;
                }
                let pos = (column & 1) == 1;
                if pos != (coord > 0.0) {
                    continue;
                }
                let scalar = coord.abs() as f64;

                if extremum_rotation_delta != 0.0 {
                    let mut center_x = extremum_translation_delta.x as f64;
                    let mut center_y = extremum_translation_delta.y as f64;
                    let mut angle = extremum_rotation_delta as f64;
                    if center_x != 0.0 || center_y != 0.0 {
                        let (s, c) = angle.sin_cos();
                        let one_minus_c = 1.0 - c;
                        if one_minus_c != 0.0 {
                            let s_over_one_minus_c = s / one_minus_c;
                            let new_center_x = (center_x - center_y * s_over_one_minus_c) * 0.5;
                            let new_center_y = (center_y + center_x * s_over_one_minus_c) * 0.5;
                            center_x = new_center_x;
                            center_y = new_center_y;
                        }
                    }
                    angle *= scalar;
                    transform *= Transform::rotation_around_center(angle, center_x, center_y);
                    is_translate_only = false;
                } else {
                    // Just scale the translate. If we have not rotated the
                    // matrix yet, `pre_translate` is equivalent to `translate`
                    // but saves a lot of multiplications.
                    if is_translate_only {
                        transform = transform.pre_translate(
                            extremum_translation_delta.x as f64 * scalar,
                            extremum_translation_delta.y as f64 * scalar,
                        );
                    } else {
                        transform = transform.translate(
                            extremum_translation_delta.x as f64 * scalar,
                            extremum_translation_delta.y as f64 * scalar,
                        );
                    }
                }
            }

            if is_translate_only {
                transforms[row] = transforms[row].pre_translate(transform.dx, transform.dy);
            } else {
                transforms[row] = transform * transforms[row];
            }
        }
    }

    for (row, delta) in master_rotation_indices.iter().zip(master_rotation_deltas) {
        let row = row.get() as usize;
        let Some(transform) = transforms.get_mut(row) else {
            break;
        };
        *transform = Transform::rotation(delta.get() as f64) * *transform;
    }

    for (row, delta) in master_translation_indices
        .iter()
        .zip(master_translation_deltas)
    {
        let row = row.get() as usize;
        let delta = delta.get();
        let Some(transform) = transforms.get_mut(row) else {
            break;
        };
        *transform = transform.pre_translate(delta.x as f64, delta.y as f64);
    }

    Ok(())
}
