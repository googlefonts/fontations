//! Fast loading of glyf outlines for autohinting.

use super::super::glyf::{Outline as GlyfOutline, Outlines as GlyfOutlines};
use super::outline::{Contour, Outline, Point};
use crate::outline::DrawError;
use core::ops::RangeInclusive;
use raw::{
    tables::glyf::{
        Anchor, CompositeGlyph, CompositeGlyphFlags, Glyph, PointFlags, PointWithFlags, SimpleGlyph,
    },
    types::{F2Dot14, Fixed, GlyphId},
    ReadError,
};
use read_fonts::{
    tables::glyf::PointMarker,
    tables::gvar::{GlyphDelta, Gvar},
    tables::variations::TupleVariation,
};
type PointI32 = raw::types::Point<i32>;
type PointFixed = raw::types::Point<Fixed>;

impl PointWithFlags<i32> for Point {
    fn x(&self) -> i32 {
        self.fx
    }

    fn y(&self) -> i32 {
        self.fy
    }

    fn x_mut(&mut self) -> &mut i32 {
        &mut self.fx
    }

    fn y_mut(&mut self) -> &mut i32 {
        &mut self.fy
    }

    fn flags(&self) -> PointFlags {
        self.flags
    }

    fn flags_mut(&mut self) -> &mut PointFlags {
        &mut self.flags
    }
}

impl Outline {
    pub(crate) fn fill_from_glyf<'a>(
        &mut self,
        outlines: &GlyfOutlines<'a>,
        outline: &GlyfOutline<'a>,
        coords: &'a [F2Dot14],
    ) -> Result<i32, DrawError> {
        self.points.clear();
        // self.points.try_reserve(outline.points + 4);
        // for point in self.points.as_mut_slice() {
        //     *point = Default::default();
        // }
        self.points.resize(outline.points);
        self.contours.clear();
        self.contours.resize(outline.contours);
        let is_var = outlines.gvar.is_some() && !coords.is_empty();
        let mut temp_mem_size = 4;
        if is_var {
            // deltas, iup_buffer
            temp_mem_size += outline.max_simple_points * core::mem::size_of::<PointFixed>() * 2;
            // composite deltas
            temp_mem_size += outline.max_component_delta_stack * core::mem::size_of::<PointFixed>();
            // temporary flags buffer
            temp_mem_size += outline.max_simple_points * core::mem::size_of::<PointFlags>();
        }
        use super::super::memory;
        memory::with_temporary_memory(temp_mem_size, |buf| {
            let (var_deltas, var_iup_buffer, var_composite_deltas, var_flags, _buf) = if is_var {
                let (deltas, buf) = memory::alloc_slice(buf, outline.max_simple_points).unwrap();
                let (iup_buffer, buf) =
                    memory::alloc_slice(buf, outline.max_simple_points).unwrap();
                let (composite_deltas, buf) =
                    memory::alloc_slice(buf, outline.max_component_delta_stack).unwrap();
                let (flags, buf) = memory::alloc_slice(buf, outline.max_simple_points).unwrap();
                (deltas, iup_buffer, composite_deltas, flags, buf)
            } else {
                (
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    buf,
                )
            };
            let mut loader = GlyfLoader {
                coords: if is_var { coords } else { &[] },
                points: self.points.as_mut_slice(),
                contours: self.contours.as_mut_slice(),
                var_deltas,
                var_iup_buffer,
                var_composite_deltas,
                var_flags,
                n_points: 0,
                n_contours: 0,
                n_component_deltas: 0,
                glyf: outlines,
                phantom: [PointI32::default(); 4],
            };
            loader.load(&outline.glyph, outline.glyph_id, 0)?;
            let n_points = loader.n_points;
            let pp0x = loader.phantom[0].x;
            let advance = loader.phantom[1].x - loader.phantom[0].x;
            self.points.truncate(n_points);
            if pp0x != 0 {
                for point in self.points.as_mut_slice() {
                    point.fx -= pp0x;
                }
            }
            Ok(advance)
        })
    }
}

const PHANTOM_POINT_COUNT: usize = 4;
const GLYF_COMPOSITE_RECURSION_LIMIT: usize = 64;

struct GlyfLoader<'a> {
    coords: &'a [F2Dot14],
    points: &'a mut [Point],
    contours: &'a mut [Contour],
    var_deltas: &'a mut [PointFixed],
    var_iup_buffer: &'a mut [PointFixed],
    var_flags: &'a mut [PointFlags],
    var_composite_deltas: &'a mut [PointFixed],
    n_points: usize,
    n_contours: usize,
    n_component_deltas: usize,
    glyf: &'a GlyfOutlines<'a>,
    phantom: [PointI32; 4],
}

impl<'a> GlyfLoader<'a> {
    fn load(
        &mut self,
        glyph: &Option<Glyph>,
        glyph_id: GlyphId,
        recurse_depth: usize,
    ) -> Result<(), DrawError> {
        if recurse_depth > GLYF_COMPOSITE_RECURSION_LIMIT {
            return Err(DrawError::RecursionLimitExceeded(glyph_id));
        }
        let bounds = match &glyph {
            Some(glyph) => [glyph.x_min(), glyph.x_max(), glyph.y_min(), glyph.y_max()],
            _ => [0; 4],
        };
        let lsb = self.glyf.glyph_metrics.lsb(glyph_id, &[]);
        let advance = self.glyf.glyph_metrics.advance_width(glyph_id, &[]);
        let [ascent, descent] = [0, 0]; //outlines.os2_vmetrics.map(|x| x as i32);
        let tsb = ascent - bounds[3] as i32;
        let vadvance = ascent - descent;
        // The four "phantom" points as computed by FreeType.
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttgload.c#L1365>
        // horizontal:
        self.phantom[0].x = bounds[0] as i32 - lsb;
        self.phantom[0].y = 0;
        self.phantom[1].x = self.phantom[0].x + advance;
        self.phantom[1].y = 0;
        // vertical:
        self.phantom[2].x = 0;
        self.phantom[2].y = bounds[3] as i32 + tsb;
        self.phantom[3].x = 0;
        self.phantom[3].y = self.phantom[2].y - vadvance;
        match glyph {
            Some(Glyph::Simple(simple)) => self.load_simple(simple, glyph_id),
            Some(Glyph::Composite(composite)) => {
                self.load_composite(composite, glyph_id, recurse_depth)
            }
            None => self.load_empty(glyph_id),
        }
    }

    fn load_empty(&mut self, glyph_id: GlyphId) -> Result<(), DrawError> {
        // Roughly corresponds to the FreeType code at
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttgload.c#L1572>
        if self.glyf.gvar.is_some() && !self.coords.is_empty() {
            if let Ok(Some(deltas)) = self.glyf.gvar.as_ref().unwrap().phantom_point_deltas(
                &self.glyf.glyf,
                &self.glyf.loca,
                self.coords,
                glyph_id,
            ) {
                self.phantom[0] += deltas[0].map(Fixed::to_i32);
                self.phantom[1] += deltas[1].map(Fixed::to_i32);
            }
        }
        Ok(())
    }

    fn load_simple(&mut self, glyph: &SimpleGlyph, glyph_id: GlyphId) -> Result<(), DrawError> {
        let points_start = self.n_points;
        let point_count = glyph.num_points();
        let points_end = points_start + point_count + PHANTOM_POINT_COUNT;
        let points_range = points_start..points_end;
        let points = &mut self.points[points_range.clone()];
        glyph.read_points_with_flags_fast(&mut points[0..point_count])?;
        let contours_start = self.n_contours;
        let contour_end_pts = glyph.end_pts_of_contours();
        let contour_count = contour_end_pts.len();
        let contours_end = contours_start + contour_count;
        let contours = &mut self.contours[contours_start..contours_end];
        // Read the contour end points, ensuring that they are properly
        // ordered.
        let mut last_end_pt = 0;
        for (end_pt, contour) in contour_end_pts.iter().zip(contours.iter_mut()) {
            let end_pt = end_pt.get();
            if end_pt < last_end_pt {
                return Err(ReadError::MalformedData(
                    "unordered contour end points in TrueType glyph",
                )
                .into());
            }
            contour.first_ix = last_end_pt;
            if last_end_pt != 0 {
                contour.first_ix += 1;
            }
            last_end_pt = end_pt;
            contour.last_ix = end_pt;
        }
        self.n_points += point_count;
        self.n_contours += contour_count;
        if !self.coords.is_empty() {
            let gvar = self.glyf.gvar.as_ref().unwrap();
            let phantom_start = points_start + point_count;
            for (point, phantom) in self.points[phantom_start..].iter_mut().zip(&self.phantom) {
                point.fx = phantom.x;
                point.fy = phantom.y;
            }
            let simple_count = point_count + PHANTOM_POINT_COUNT;
            let points = self.points.get_mut(points_range).unwrap();
            let deltas = self.var_deltas.get_mut(..simple_count).unwrap();
            let iup_buffer = self.var_iup_buffer.get_mut(..simple_count).unwrap();
            let flags = self.var_flags.get_mut(..simple_count).unwrap();
            if simple_deltas(
                gvar,
                glyph_id,
                points,
                flags,
                contours,
                self.coords,
                iup_buffer,
                deltas,
            )
            .is_ok()
            {
                for (point, delta) in points.iter_mut().zip(deltas) {
                    point.fx += delta.x.to_i32();
                    point.fy += delta.y.to_i32();
                }
                for (point, phantom) in self.points[phantom_start..].iter().zip(&mut self.phantom) {
                    phantom.x = point.fx;
                    phantom.y = point.fy;
                }
            }
        }
        if points_start != 0 {
            for contour in contours {
                contour.first_ix += points_start as u16;
                contour.last_ix += points_start as u16;
            }
        }
        Ok(())
    }

    fn load_composite(
        &mut self,
        glyph: &CompositeGlyph,
        glyph_id: GlyphId,
        recurse_depth: usize,
    ) -> Result<(), DrawError> {
        let delta_base = self.n_component_deltas;
        let mut have_deltas = false;
        if !self.coords.is_empty() {
            let gvar = self.glyf.gvar.as_ref().unwrap();
            let count = glyph.components().count() + PHANTOM_POINT_COUNT;
            let deltas = self
                .var_composite_deltas
                .get_mut(delta_base..delta_base + count)
                .unwrap();
            if composite_deltas(gvar, glyph_id, self.coords, &mut deltas[..]).is_ok() {
                // Apply deltas to phantom points.
                for (phantom, delta) in self
                    .phantom
                    .iter_mut()
                    .zip(&deltas[deltas.len() - PHANTOM_POINT_COUNT..])
                {
                    *phantom += delta.map(Fixed::to_i32);
                }
                have_deltas = true;
            }
            self.n_component_deltas += count;
        }
        for (i, component) in glyph.components().enumerate() {
            let phantom = self.phantom;
            let start_point = self.n_points;
            let component_glyph = self
                .glyf
                .loca
                .get_glyf(component.glyph.into(), &self.glyf.glyf)?;
            self.load(&component_glyph, component.glyph.into(), recurse_depth + 1)?;
            let end_point = self.n_points;
            if !component
                .flags
                .contains(CompositeGlyphFlags::USE_MY_METRICS)
            {
                // If the USE_MY_METRICS flag is missing, we restore the phantom points we
                // saved at the start of the loop.
                self.phantom = phantom;
            }
            // Prepares the transform components for our conversion math below.
            fn scale_component(x: F2Dot14) -> Fixed {
                Fixed::from_bits(x.to_bits() as i32 * 4)
            }
            let xform = &component.transform;
            let xx = scale_component(xform.xx);
            let yx = scale_component(xform.yx);
            let xy = scale_component(xform.xy);
            let yy = scale_component(xform.yy);
            let have_xform = component.flags.intersects(
                CompositeGlyphFlags::WE_HAVE_A_SCALE
                    | CompositeGlyphFlags::WE_HAVE_AN_X_AND_Y_SCALE
                    | CompositeGlyphFlags::WE_HAVE_A_TWO_BY_TWO,
            );
            if have_xform {
                let points = &mut self.points[start_point..end_point];
                for point in points {
                    let fx = Fixed::from_bits(point.fx);
                    let fy = Fixed::from_bits(point.fy);
                    let x = fx * xx + fy * xy;
                    let y = fx * yx + fy * yy;
                    point.fx = x.to_bits();
                    point.fy = y.to_bits();
                }
            }
            let anchor_offset = match component.anchor {
                Anchor::Offset { x, y } => {
                    let (mut x, mut y) = (x as i32, y as i32);
                    if have_xform
                        && component.flags
                            & (CompositeGlyphFlags::SCALED_COMPONENT_OFFSET
                                | CompositeGlyphFlags::UNSCALED_COMPONENT_OFFSET)
                            == CompositeGlyphFlags::SCALED_COMPONENT_OFFSET
                    {
                        // According to FreeType, this algorithm is a "guess"
                        // and works better than the one documented by Apple.
                        // https://github.com/freetype/freetype/blob/b1c90733ee6a04882b133101d61b12e352eeb290/src/truetype/ttgload.c#L1259
                        fn hypot(a: Fixed, b: Fixed) -> Fixed {
                            let a = a.to_bits().abs();
                            let b = b.to_bits().abs();
                            Fixed::from_bits(if a > b {
                                a + ((3 * b) >> 3)
                            } else {
                                b + ((3 * a) >> 3)
                            })
                        }
                        // FreeType uses a fixed point multiplication here.
                        x = (Fixed::from_bits(x) * hypot(xx, xy)).to_bits();
                        y = (Fixed::from_bits(y) * hypot(yy, yx)).to_bits();
                    }
                    if have_deltas {
                        let delta = self
                            .var_composite_deltas
                            .get(delta_base + i)
                            .copied()
                            .unwrap_or_default();
                        // For composite glyphs, we copy FreeType and round off
                        // the fractional parts of deltas.
                        x += delta.x.to_i32();
                        y += delta.y.to_i32();
                    }
                    (x, y)
                }
                Anchor::Point {
                    base: _,
                    component: _,
                } => {
                    // panic!("don't support Anchor::Point");
                    (0, 0)
                }
            };
            if anchor_offset.0 != 0 || anchor_offset.1 != 0 {
                for point in &mut self.points[start_point..end_point] {
                    point.fx += anchor_offset.0;
                    point.fy += anchor_offset.1;
                }
            }
        }
        self.n_component_deltas = delta_base;
        Ok(())
    }
}

/// Compute a set of deltas for the component offsets of a composite glyph.
///
/// Interpolation is meaningless for component offsets so this is a
/// specialized function that skips the expensive bits.
pub(super) fn composite_deltas(
    gvar: &Gvar,
    glyph_id: GlyphId,
    coords: &[F2Dot14],
    deltas: &mut [PointFixed],
) -> Result<(), ReadError> {
    compute_deltas_for_glyph(gvar, glyph_id, coords, deltas, |scalar, tuple, deltas| {
        for tuple_delta in tuple.deltas() {
            let ix = tuple_delta.position as usize;
            if let Some(delta) = deltas.get_mut(ix) {
                *delta += tuple_delta.apply_scalar(scalar);
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Compute a set of deltas for the points in a simple glyph.
///
/// This function will use interpolation to infer missing deltas for tuples
/// that contain sparse sets. The `iup_buffer` buffer is temporary storage
/// used for this and the length must be >= glyph.points.len().
pub(super) fn simple_deltas(
    gvar: &Gvar,
    glyph_id: GlyphId,
    points: &[Point],
    flags: &mut [PointFlags],
    contours: &[Contour],
    coords: &[F2Dot14],
    iup_buffer: &mut [PointFixed],
    deltas: &mut [PointFixed],
) -> Result<(), ReadError> {
    if iup_buffer.len() < points.len() || points.len() < PHANTOM_POINT_COUNT {
        return Err(ReadError::InvalidArrayLen);
    }
    compute_deltas_for_glyph(gvar, glyph_id, coords, deltas, |scalar, tuple, deltas| {
        // Infer missing deltas by interpolation.
        // Prepare our working buffer by converting the points to 16.16
        // and clearing the HAS_DELTA flags.
        flags.fill(PointFlags::default());
        for (point, iup_point) in points.iter().zip(&mut iup_buffer[..]) {
            iup_point.x = Fixed::from_i32(point.fx);
            iup_point.y = Fixed::from_i32(point.fy);
        }
        tuple.accumulate_sparse_deltas(iup_buffer, flags, scalar)?;
        interpolate_deltas(points, flags, contours, &mut iup_buffer[..])
            .ok_or(ReadError::OutOfBounds)?;
        for ((delta, point), iup_point) in deltas.iter_mut().zip(points).zip(iup_buffer.iter()) {
            *delta += *iup_point - PointI32::new(point.fx, point.fy).map(Fixed::from);
        }
        Ok(())
    })?;
    Ok(())
}

/// The common parts of simple and complex glyph processing
fn compute_deltas_for_glyph(
    gvar: &Gvar,
    glyph_id: GlyphId,
    coords: &[F2Dot14],
    deltas: &mut [PointFixed],
    mut apply_tuple_missing_deltas_fn: impl FnMut(
        Fixed,
        TupleVariation<GlyphDelta>,
        &mut [PointFixed],
    ) -> Result<(), ReadError>,
) -> Result<(), ReadError> {
    deltas.fill(Default::default());
    let Ok(Some(var_data)) = gvar.glyph_variation_data(glyph_id) else {
        // Empty variation data for a glyph is not an error.
        return Ok(());
    };
    for (tuple, scalar) in var_data.active_tuples_at(coords) {
        // Fast path: tuple contains all points, we can simply accumulate
        // the deltas directly.
        if tuple.has_deltas_for_all_points() {
            tuple.accumulate_dense_deltas(deltas, scalar)?;
        } else {
            // Slow path is, annoyingly, different for simple vs composite
            // so let the caller handle it
            apply_tuple_missing_deltas_fn(scalar, tuple, deltas)?;
        }
    }
    Ok(())
}

/// Interpolate points without delta values, similar to the IUP hinting
/// instruction.
///
/// Modeled after the FreeType implementation:
/// <https://github.com/freetype/freetype/blob/bbfcd79eacb4985d4b68783565f4b494aa64516b/src/truetype/ttgxvar.c#L3881>
fn interpolate_deltas(
    points: &[Point],
    flags: &[PointFlags],
    contours: &[Contour],
    out_points: &mut [PointFixed],
) -> Option<()> {
    let mut jiggler = Jiggler { points, out_points };
    let mut point_ix = 0usize;
    for contour in contours {
        let end_point_ix = contour.last();
        let first_point_ix = contour.first();
        // Search for first point that has a delta.
        while point_ix <= end_point_ix && !flags.get(point_ix)?.has_marker(PointMarker::HAS_DELTA) {
            point_ix += 1;
        }
        // If we didn't find any deltas, no variations in the current tuple
        // apply, so skip it.
        if point_ix > end_point_ix {
            continue;
        }
        let first_delta_ix = point_ix;
        let mut cur_delta_ix = point_ix;
        point_ix += 1;
        // Search for next point that has a delta...
        while point_ix <= end_point_ix {
            if flags.get(point_ix)?.has_marker(PointMarker::HAS_DELTA) {
                // ... and interpolate intermediate points.
                jiggler.interpolate(
                    cur_delta_ix + 1..=point_ix - 1,
                    RefPoints(cur_delta_ix, point_ix),
                )?;
                cur_delta_ix = point_ix;
            }
            point_ix += 1;
        }
        // If we only have a single delta, shift the contour.
        if cur_delta_ix == first_delta_ix {
            jiggler.shift(first_point_ix..=end_point_ix, cur_delta_ix)?;
        } else {
            // Otherwise, handle remaining points at beginning and end of
            // contour.
            jiggler.interpolate(
                cur_delta_ix + 1..=end_point_ix,
                RefPoints(cur_delta_ix, first_delta_ix),
            )?;
            if first_delta_ix > 0 {
                jiggler.interpolate(
                    first_point_ix..=first_delta_ix - 1,
                    RefPoints(cur_delta_ix, first_delta_ix),
                )?;
            }
        }
    }
    Some(())
}

struct RefPoints(usize, usize);

struct Jiggler<'a> {
    points: &'a [Point],
    out_points: &'a mut [PointFixed],
}

impl Jiggler<'_> {
    /// Shift the coordinates of all points in the specified range using the
    /// difference given by the point at `ref_ix`.
    ///
    /// Modeled after the FreeType implementation: <https://github.com/freetype/freetype/blob/bbfcd79eacb4985d4b68783565f4b494aa64516b/src/truetype/ttgxvar.c#L3776>
    fn shift(&mut self, range: RangeInclusive<usize>, ref_ix: usize) -> Option<()> {
        let ref_in = self
            .points
            .get(ref_ix)
            .map(|p| PointI32::new(p.fx, p.fy))?
            .map(Fixed::from_i32);
        let ref_out = self.out_points.get(ref_ix)?;
        let delta = *ref_out - ref_in;
        if delta.x == Fixed::ZERO && delta.y == Fixed::ZERO {
            return Some(());
        }
        // Apply the reference point delta to the entire range excluding the
        // reference point itself which would apply the delta twice.
        for out_point in self.out_points.get_mut(*range.start()..ref_ix)? {
            *out_point += delta;
        }
        for out_point in self.out_points.get_mut(ref_ix + 1..=*range.end())? {
            *out_point += delta;
        }
        Some(())
    }

    /// Interpolate the coordinates of all points in the specified range using
    /// `ref1_ix` and `ref2_ix` as the reference point indices.
    ///
    /// Modeled after the FreeType implementation: <https://github.com/freetype/freetype/blob/bbfcd79eacb4985d4b68783565f4b494aa64516b/src/truetype/ttgxvar.c#L3813>
    ///
    /// For details on the algorithm, see: <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#inferred-deltas-for-un-referenced-point-numbers>
    fn interpolate(&mut self, range: RangeInclusive<usize>, ref_points: RefPoints) -> Option<()> {
        if range.is_empty() {
            return Some(());
        }
        // FreeType uses pointer tricks to handle x and y coords with a single piece of code.
        // Try a macro instead.
        macro_rules! interp_coord {
            ($coord:ident, $pcoord:ident) => {
                let RefPoints(mut ref1_ix, mut ref2_ix) = ref_points;
                if self.points.get(ref1_ix)?.$pcoord > self.points.get(ref2_ix)?.$pcoord {
                    core::mem::swap(&mut ref1_ix, &mut ref2_ix);
                }
                let in1 = Fixed::from(self.points.get(ref1_ix)?.$pcoord);
                let in2 = Fixed::from(self.points.get(ref2_ix)?.$pcoord);
                let out1 = self.out_points.get(ref1_ix)?.$coord;
                let out2 = self.out_points.get(ref2_ix)?.$coord;
                // If the reference points have the same coordinate but different delta,
                // inferred delta is zero. Otherwise interpolate.
                if in1 != in2 || out1 == out2 {
                    let scale = if in1 != in2 {
                        (out2 - out1) / (in2 - in1)
                    } else {
                        Fixed::ZERO
                    };
                    let d1 = out1 - in1;
                    let d2 = out2 - in2;
                    for (point, out_point) in self
                        .points
                        .get(range.clone())?
                        .iter()
                        .zip(self.out_points.get_mut(range.clone())?)
                    {
                        let mut out = Fixed::from(point.$pcoord);
                        if out <= in1 {
                            out += d1;
                        } else if out >= in2 {
                            out += d2;
                        } else {
                            out = out1 + (out - in1) * scale;
                        }
                        out_point.$coord = out;
                    }
                }
            };
        }
        interp_coord!(x, fx);
        interp_coord!(y, fy);
        Some(())
    }
}
