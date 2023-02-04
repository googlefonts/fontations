use read_fonts::{
    tables::glyf::{PointFlags, PointMarker},
    tables::gvar::{GlyphDelta, Gvar},
    types::{F26Dot6, F2Dot14, Fixed, GlyphId, Point},
    ReadError,
};

/// Compute a set of deltas for the component offsets of a composite glyph.
///
/// Interpolation is meaningless for component offsets so this is a
/// specialized function that skips the expensive bits.
pub fn composite_glyph(
    gvar: &Gvar,
    glyph_id: GlyphId,
    coords: &[F2Dot14],
    deltas: &mut [Point<Fixed>],
) -> Result<(), ReadError> {
    for delta in deltas.iter_mut() {
        *delta = Default::default();
    }
    let Ok(var_data) = gvar.glyph_variation_data(glyph_id) else {
        // Empty variation data for a glyph is not an error.
        return Ok(());
    };
    for tuple in var_data.tuples() {
        let Some(scalar) = tuple.compute_scalar(coords) else {
            continue;
        };
        if tuple.all_points() {
            for (delta, tuple_delta) in deltas.iter_mut().zip(tuple.deltas()) {
                *delta += tuple_delta.apply_scalar(scalar);
            }
        } else {
            for tuple_delta in tuple.deltas() {
                let ix = tuple_delta.position as usize;
                if let Some(delta) = deltas.get_mut(ix) {
                    *delta += tuple_delta.apply_scalar(scalar);
                }
            }
        }
    }
    Ok(())
}

pub struct SimpleGlyph<'a> {
    pub points: &'a [Point<i32>],
    pub flags: &'a mut [PointFlags],
    pub contours: &'a [u16],
}

/// Compute a set of deltas for the points in a simple glyph.
///
/// This function will use interpolation to infer missing deltas for tuples
/// that contain sparse sets. The `working_points` buffer is temporary storage
/// used for this and the length must be >= glyph.points.len().
pub fn simple_glyph(
    gvar: &Gvar,
    glyph_id: GlyphId,
    coords: &[F2Dot14],
    glyph: SimpleGlyph,
    working_points: &mut [Point<Fixed>],
    deltas: &mut [Point<Fixed>],
) -> Result<(), ReadError> {
    for delta in deltas.iter_mut() {
        *delta = Default::default();
    }
    let Ok(var_data) = gvar.glyph_variation_data(glyph_id) else {
        // Empty variation data for a glyph is not an error.
        return Ok(());
    };
    let SimpleGlyph {
        points,
        flags,
        contours,
    } = glyph;
    // We don't apply variations to the phantom points. A properly constructed
    // variable font contains an HVAR table which would have already applied the
    // adjustments we generate here.
    let len_without_phantom = points.len() - 4;
    for tuple in var_data.tuples() {
        let Some(scalar) = tuple.compute_scalar(coords) else {
            continue;
        };
        if tuple.all_points() {
            // When a tuple contains all points, we can simply accumulate the deltas directly.
            for (delta, tuple_delta) in deltas[0..len_without_phantom]
                .iter_mut()
                .zip(tuple.deltas())
            {
                *delta += tuple_delta.apply_scalar(scalar);
            }
        } else {
            // Otherwise, we need to infer missing deltas by interpolation.
            // Prepare our working buffer and clear the HAS_DELTA flags.
            for ((flag, point), working_point) in
                flags.iter_mut().zip(points).zip(&mut working_points[..])
            {
                *working_point = point.map(Fixed::from_i32);
                flag.clear_marker(PointMarker::HAS_DELTA);
            }
            for tuple_delta in tuple.deltas() {
                let ix = tuple_delta.position as usize;
                if let (Some(flag), Some(point)) = (flags.get_mut(ix), working_points.get_mut(ix)) {
                    flag.set_marker(PointMarker::HAS_DELTA);
                    *point += tuple_delta.apply_scalar(scalar);
                }
            }
            interpolate_deltas(points, flags, contours, &mut working_points[..])
                .ok_or(ReadError::OutOfBounds)?;
            for ((delta, point), working_point) in deltas[..len_without_phantom]
                .iter_mut()
                .zip(points)
                .zip(working_points.iter())
            {
                *delta += *working_point - point.map(Fixed::from_i32);
            }
        }
    }
    Ok(())
}

fn interpolate_deltas(
    points: &[Point<i32>],
    flags: &[PointFlags],
    contours: &[u16],
    out_points: &mut [Point<Fixed>],
) -> Option<()> {
    if contours.is_empty() {
        return Some(());
    }
    let mut point_ix = 0usize;
    for &end_point_ix in contours {
        let end_point_ix = end_point_ix as usize;
        let first_point_ix = point_ix;
        while point_ix <= end_point_ix && flags.get(point_ix)?.has_marker(PointMarker::HAS_DELTA) {
            point_ix += 1;
        }
        if point_ix <= end_point_ix {
            let first_delta_ix = point_ix;
            let mut cur_delta_ix = point_ix;
            point_ix += 1;
            while point_ix <= end_point_ix {
                if flags.get(point_ix)?.has_marker(PointMarker::HAS_DELTA) {
                    interpolate_range(
                        points,
                        cur_delta_ix + 1,
                        point_ix - 1,
                        cur_delta_ix,
                        point_ix,
                        out_points,
                    );
                    cur_delta_ix = point_ix;
                }
                point_ix += 1;
            }
            if cur_delta_ix == first_delta_ix {
                shift_range(
                    points,
                    first_point_ix,
                    end_point_ix,
                    cur_delta_ix,
                    out_points,
                )?;
            } else {
                interpolate_range(
                    points,
                    cur_delta_ix + 1,
                    end_point_ix,
                    cur_delta_ix,
                    first_delta_ix,
                    out_points,
                )?;
                if first_delta_ix > 0 {
                    interpolate_range(
                        points,
                        first_point_ix,
                        first_delta_ix - 1,
                        cur_delta_ix,
                        first_delta_ix,
                        out_points,
                    )?;
                }
            }
        }
    }
    Some(())
}

/// Shifts a range of points by the difference in the given reference
/// point.
fn shift_range(
    points: &[Point<i32>],
    start: usize,
    end: usize,
    ref_: usize,
    out_points: &mut [Point<Fixed>],
) -> Option<()> {
    let ref_in = points.get(ref_)?.map(Fixed::from_i32);
    let ref_out = out_points.get(ref_)?;
    let delta = *ref_out - ref_in;
    if delta.x == Fixed::ZERO && delta.y == Fixed::ZERO {
        return Some(());
    }
    for out_point in out_points.get_mut(start..ref_)? {
        *out_point += delta;
    }
    for out_point in out_points.get_mut(ref_ + 1..=end)? {
        *out_point += delta;
    }
    Some(())
}

/// Generates inferred deltas by interpolating between the given range of points.
///
/// For details on the algorithm, see: <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#inferred-deltas-for-un-referenced-point-numbers>
fn interpolate_range(
    points: &[Point<i32>],
    start: usize,
    end: usize,
    ref1: usize,
    ref2: usize,
    out_points: &mut [Point<Fixed>],
) -> Option<()> {
    if start > end {
        return Some(());
    }
    // FreeType uses pointer tricks to handle x and y coords with a single piece of code.
    // Try a macro instead.
    macro_rules! interp_coord {
        ($coord:ident) => {
            let mut ref1 = ref1;
            let mut ref2 = ref2;
            if points.get(ref1)?.$coord > points.get(ref2)?.$coord {
                core::mem::swap(&mut ref1, &mut ref2);
            }
            let in1 = Fixed::from_i32(points.get(ref1)?.$coord);
            let in2 = Fixed::from_i32(points.get(ref2)?.$coord);
            let out1 = out_points.get(ref1)?.$coord;
            let out2 = out_points.get(ref2)?.$coord;
            let d1 = out1 - in1;
            let d2 = out2 - in2;
            // If the reference points have the same coordinate but different delta,
            // inferred delta is zero. Otherwise interpolate.
            if in1 != in2 || out1 == out2 {
                let scale = if in1 != in2 {
                    (out2 - out1) / (in2 - in1)
                } else {
                    Fixed::ZERO
                };
                let range = start..=end;
                for (point, out_point) in points
                    .get(range.clone())?
                    .iter()
                    .zip(out_points.get_mut(range.clone())?)
                {
                    let mut out = Fixed::from_i32(point.$coord);
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
    interp_coord!(x);
    interp_coord!(y);
    Some(())
}
