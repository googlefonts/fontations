//! Computation of glyph point deltas from the `gvar` table.
//!
//! Tuples in a glyph's variation data come in two shapes. A *dense* tuple
//! carries a delta for every point and is simply accumulated. A *sparse* tuple
//! carries deltas for a subset, and the deltas for the remaining points have to
//! be inferred by interpolating between the nearest referenced points on either
//! side, in the manner of the `IUP` hinting instruction.
//!
//! See [inferred deltas for un-referenced point numbers](https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#inferred-deltas-for-un-referenced-point-numbers).

use core::ops::RangeInclusive;

use super::{GlyphDelta, Gvar};
use crate::{
    tables::{
        glyf::{PointCoord, PointFlags, PointMarker, PHANTOM_POINT_COUNT},
        variations::TupleVariation,
    },
    types::{F2Dot14, Fixed, GlyphId, Point},
    ReadError,
};

/// Caller-provided storage for [`Gvar::simple_deltas`].
///
/// Both slices must be at least as long as the glyph's point count including
/// the phantom points.
pub struct DeltaBuffers<'a, D: PointCoord> {
    /// Receives the computed deltas.
    pub deltas: &'a mut [Point<D>],
    /// Working space used to interpolate the deltas of points that a sparse
    /// tuple does not reference.
    pub iup: &'a mut [Point<D>],
}

impl Gvar<'_> {
    /// Computes the deltas for the points of a simple glyph at the given
    /// location in variation space.
    ///
    /// Deltas for points that a sparse tuple does not reference are inferred by
    /// interpolation, so this needs the glyph's points and contour end points
    /// in addition to the buffers it writes.
    ///
    /// * `points` and `contours` describe the unvaried glyph. `points` must
    ///   include the four phantom points.
    /// * `flags` is used as scratch: the [`PointMarker::HAS_DELTA`] marker is
    ///   cleared and set as tuples are processed. Its length must match
    ///   `points`.
    /// * `buffers` supplies the output and interpolation storage.
    ///
    /// Returns `true` if the glyph has variation data, and `false` if it does
    /// not — in which case the deltas are zeroed. Note that they are zeroed
    /// before anything else happens, so they never retain values from a
    /// previous call, whatever this returns.
    pub fn simple_deltas<C, D>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        points: &[Point<C>],
        flags: &mut [PointFlags],
        contours: &[u16],
        buffers: &mut DeltaBuffers<'_, D>,
    ) -> Result<bool, ReadError>
    where
        C: PointCoord,
        D: PointCoord + From<C>,
    {
        let DeltaBuffers { deltas, iup } = buffers;
        if iup.len() < points.len() || points.len() < PHANTOM_POINT_COUNT {
            return Err(ReadError::InvalidArrayLen);
        }
        self.compute_deltas(glyph_id, coords, deltas, |scalar, tuple, deltas| {
            // Prepare the working buffer by converting the points to 16.16 and
            // clearing the markers left by the previous tuple.
            for ((flag, point), iup_point) in flags.iter_mut().zip(points).zip(&mut iup[..]) {
                *iup_point = point.map(D::from);
                flag.clear_marker(PointMarker::HAS_DELTA);
            }
            tuple.accumulate_sparse_deltas(iup, flags, scalar)?;
            interpolate_deltas(points, flags, contours, &mut iup[..])
                .ok_or(ReadError::OutOfBounds)?;
            for ((delta, point), iup_point) in deltas.iter_mut().zip(points).zip(iup.iter()) {
                *delta += *iup_point - point.map(D::from);
            }
            Ok(())
        })
    }

    /// Computes the deltas for the component offsets of a composite glyph at
    /// the given location in variation space.
    ///
    /// Interpolation is meaningless for component offsets, so this skips the
    /// expensive part of [`Gvar::simple_deltas`] and needs no scratch.
    ///
    /// `deltas` must have one entry per component plus four for the phantom
    /// points.
    ///
    /// Returns `true` if the glyph has variation data, and `false` if it does
    /// not — in which case `deltas` is zeroed.
    pub fn composite_deltas<D: PointCoord>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        deltas: &mut [Point<D>],
    ) -> Result<bool, ReadError> {
        self.compute_deltas(glyph_id, coords, deltas, |scalar, tuple, deltas| {
            for tuple_delta in tuple.deltas() {
                let ix = tuple_delta.position as usize;
                if let Some(delta) = deltas.get_mut(ix) {
                    *delta += tuple_delta.apply_scalar(scalar);
                }
            }
            Ok(())
        })
    }

    /// The parts shared by simple and composite glyph processing.
    ///
    /// Zeroes `deltas`, then accumulates every tuple that is active at
    /// `coords`. Dense tuples are handled here; sparse tuples are passed to
    /// `apply_sparse_tuple`, which differs between the two glyph kinds.
    fn compute_deltas<D: PointCoord>(
        &self,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        deltas: &mut [Point<D>],
        mut apply_sparse_tuple: impl FnMut(
            Fixed,
            TupleVariation<GlyphDelta>,
            &mut [Point<D>],
        ) -> Result<(), ReadError>,
    ) -> Result<bool, ReadError> {
        // Always zero first: callers must never observe values left over from a
        // previous glyph, including on the paths that bail out below.
        for delta in deltas.iter_mut() {
            *delta = Default::default();
        }
        let Ok(Some(var_data)) = self.glyph_variation_data(glyph_id) else {
            // Missing or malformed variation data for a glyph is not an error.
            return Ok(false);
        };
        for (tuple, scalar) in var_data.active_tuples_at(coords) {
            if tuple.has_deltas_for_all_points() {
                // Fast path: the tuple covers every point, so the deltas can be
                // accumulated directly with no interpolation.
                tuple.accumulate_dense_deltas(deltas, scalar)?;
            } else {
                apply_sparse_tuple(scalar, tuple, deltas)?;
            }
        }
        Ok(true)
    }
}

/// Interpolates the points that the current tuple did not reference, in the
/// manner of the `IUP` hinting instruction.
///
/// Points carrying an explicit delta are marked with [`PointMarker::HAS_DELTA`]
/// in `flags`.
///
/// Modeled after the FreeType implementation:
/// <https://github.com/freetype/freetype/blob/bbfcd79eacb4985d4b68783565f4b494aa64516b/src/truetype/ttgxvar.c#L3881>
fn interpolate_deltas<C, D>(
    points: &[Point<C>],
    flags: &[PointFlags],
    contours: &[u16],
    out_points: &mut [Point<D>],
) -> Option<()>
where
    C: PointCoord,
    D: PointCoord + From<C>,
{
    let mut jiggler = Jiggler { points, out_points };
    let mut point_ix = 0usize;
    for &end_point_ix in contours {
        let end_point_ix = end_point_ix as usize;
        let first_point_ix = point_ix;
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

struct Jiggler<'a, C, D>
where
    C: PointCoord,
    D: PointCoord + From<C>,
{
    points: &'a [Point<C>],
    out_points: &'a mut [Point<D>],
}

impl<C, D> Jiggler<'_, C, D>
where
    C: PointCoord,
    D: PointCoord + From<C>,
{
    /// Shift the coordinates of all points in the specified range using the
    /// difference given by the point at `ref_ix`.
    ///
    /// Modeled after the FreeType implementation: <https://github.com/freetype/freetype/blob/bbfcd79eacb4985d4b68783565f4b494aa64516b/src/truetype/ttgxvar.c#L3776>
    fn shift(&mut self, range: RangeInclusive<usize>, ref_ix: usize) -> Option<()> {
        let ref_in = self.points.get(ref_ix)?.map(D::from);
        let ref_out = self.out_points.get(ref_ix)?;
        let delta = *ref_out - ref_in;
        if delta.x == D::zeroed() && delta.y == D::zeroed() {
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
            ($coord:ident) => {
                let RefPoints(mut ref1_ix, mut ref2_ix) = ref_points;
                if self.points.get(ref1_ix)?.$coord > self.points.get(ref2_ix)?.$coord {
                    core::mem::swap(&mut ref1_ix, &mut ref2_ix);
                }
                let in1 = D::from(self.points.get(ref1_ix)?.$coord);
                let in2 = D::from(self.points.get(ref2_ix)?.$coord);
                let out1 = self.out_points.get(ref1_ix)?.$coord;
                let out2 = self.out_points.get(ref2_ix)?.$coord;
                // If the reference points have the same coordinate but different delta,
                // inferred delta is zero. Otherwise interpolate.
                if in1 != in2 || out1 == out2 {
                    let scale = if in1 != in2 {
                        (out2 - out1) / (in2 - in1)
                    } else {
                        D::zeroed()
                    };
                    let d1 = out1 - in1;
                    let d2 = out2 - in2;
                    for (point, out_point) in self
                        .points
                        .get(range.clone())?
                        .iter()
                        .zip(self.out_points.get_mut(range.clone())?)
                    {
                        let mut out = D::from(point.$coord);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tables::{
            glyf::{Glyf, Glyph},
            loca::Loca,
        },
        FontRef, TableProvider,
    };
    use alloc::{vec, vec::Vec};

    fn make_points(tuples: &[(i32, i32)]) -> Vec<Point<i32>> {
        tuples.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    /// Seeds the working buffer the way [`Gvar::simple_deltas`] does: every
    /// point converted to 16.16 with its explicit delta already applied, and
    /// `HAS_DELTA` set for the points that carry one.
    fn make_working_points_and_flags(
        points: &[Point<i32>],
        deltas: &[Point<i32>],
    ) -> (Vec<Point<Fixed>>, Vec<PointFlags>) {
        let working_points = points
            .iter()
            .zip(deltas)
            .map(|(point, delta)| point.map(Fixed::from_i32) + delta.map(Fixed::from_i32))
            .collect();
        let flags = deltas
            .iter()
            .map(|delta| {
                let mut flags = PointFlags::default();
                if delta.x != 0 || delta.y != 0 {
                    flags.set_marker(PointMarker::HAS_DELTA);
                }
                flags
            })
            .collect();
        (working_points, flags)
    }

    /// Runs interpolation and returns the resulting x coordinates as integers.
    fn interpolated_x(
        points: &[Point<i32>],
        deltas: &[Point<i32>],
        contours: &[u16],
    ) -> Option<Vec<i32>> {
        let (mut working, flags) = make_working_points_and_flags(points, deltas);
        interpolate_deltas(points, &flags, contours, &mut working)?;
        Some(working.iter().map(|p| p.x.to_i32()).collect())
    }

    #[test]
    fn shift() {
        let points = make_points(&[(245, 630), (260, 700), (305, 680)]);
        // Single delta triggers a full contour shift.
        let deltas = make_points(&[(20, -10), (0, 0), (0, 0)]);
        let (mut working_points, flags) = make_working_points_and_flags(&points, &deltas);
        interpolate_deltas(&points, &flags, &[2], &mut working_points).unwrap();
        let expected = &[
            Point::new(265, 620).map(Fixed::from_i32),
            Point::new(280, 690).map(Fixed::from_i32),
            Point::new(325, 670).map(Fixed::from_i32),
        ];
        assert_eq!(&working_points, expected);
    }

    #[test]
    fn interpolate() {
        // Test taken from the spec:
        // https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#inferred-deltas-for-un-referenced-point-numbers
        // with a minor adjustment to account for the precision of our fixed point math.
        let points = make_points(&[(245, 630), (260, 700), (305, 680)]);
        let deltas = make_points(&[(28, -62), (0, 0), (-42, -57)]);
        let (mut working_points, flags) = make_working_points_and_flags(&points, &deltas);
        interpolate_deltas(&points, &flags, &[2], &mut working_points).unwrap();
        assert_eq!(
            working_points[1],
            Point::new(
                Fixed::from_f64(260.0 + 10.4999237060547),
                Fixed::from_f64(700.0 - 57.0)
            )
        );
    }

    /// Points below the first reference take its delta, points above the second
    /// take that one, and points between them are interpolated. Coordinates are
    /// chosen so the interpolation scale is exactly 2 and the 16.16 arithmetic
    /// is exact.
    #[test]
    fn interpolate_clamps_outside_the_reference_range() {
        //                       below    ref1    between   ref2     above
        let points = make_points(&[(0, 0), (10, 0), (15, 0), (20, 0), (30, 0)]);
        let deltas = make_points(&[(0, 0), (4, 0), (0, 0), (14, 0), (0, 0)]);
        assert_eq!(
            interpolated_x(&points, &deltas, &[4]).unwrap(),
            // 0 + d1, 10 + 4, 14 + (15-10)*2, 20 + 14, 30 + d2
            vec![4, 14, 24, 34, 44]
        );
    }

    /// A contour whose points carry no deltas at all is left exactly as it was.
    #[test]
    fn contour_without_deltas_is_untouched() {
        let points = make_points(&[(0, 0), (10, 0), (20, 0)]);
        let deltas = make_points(&[(0, 0), (0, 0), (0, 0)]);
        assert_eq!(
            interpolated_x(&points, &deltas, &[2]).unwrap(),
            vec![0, 10, 20]
        );
    }

    /// Every point having an explicit delta leaves nothing to interpolate.
    #[test]
    fn fully_referenced_contour_is_untouched() {
        let points = make_points(&[(0, 0), (10, 0), (20, 0)]);
        let deltas = make_points(&[(1, 0), (2, 0), (3, 0)]);
        assert_eq!(
            interpolated_x(&points, &deltas, &[2]).unwrap(),
            vec![1, 12, 23]
        );
    }

    /// Deltas in one contour must not leak into another.
    #[test]
    fn contours_are_independent() {
        let points = make_points(&[(0, 0), (10, 0), (20, 0), (100, 0), (110, 0), (120, 0)]);
        // Only the first contour has a delta, so it shifts as a whole while the
        // second stays put.
        let deltas = make_points(&[(5, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)]);
        assert_eq!(
            interpolated_x(&points, &deltas, &[2, 5]).unwrap(),
            vec![5, 15, 25, 100, 110, 120]
        );
    }

    /// The points before the first referenced point wrap around and use the
    /// last referenced point as their other reference.
    #[test]
    fn points_before_first_reference_wrap_around() {
        let points = make_points(&[(0, 0), (10, 0), (20, 0), (30, 0)]);
        // References at 1 and 2; points 0 and 3 fall outside them and take the
        // delta of the nearer reference.
        let deltas = make_points(&[(0, 0), (6, 0), (6, 0), (0, 0)]);
        assert_eq!(
            interpolated_x(&points, &deltas, &[3]).unwrap(),
            vec![6, 16, 26, 36]
        );
    }

    /// When both references share a coordinate but move differently the
    /// inferred delta is zero, so that coordinate is left alone.
    #[test]
    fn equal_reference_coords_with_different_deltas_infer_nothing() {
        let points = make_points(&[(10, 0), (10, 5), (10, 10)]);
        let deltas = make_points(&[(2, 0), (0, 0), (6, 0)]);
        let (mut working, flags) = make_working_points_and_flags(&points, &deltas);
        interpolate_deltas(&points, &flags, &[2], &mut working).unwrap();
        // x: both references sit at 10 but move differently, so point 1 keeps
        // its x. y: interpolated normally, and with no y deltas it stays at 5.
        assert_eq!(working[1], Point::new(10, 5).map(Fixed::from_i32));
    }

    /// A contour end point that goes backwards is skipped rather than panicking
    /// or corrupting the points around it.
    #[test]
    fn out_of_order_contour_end_is_skipped() {
        let points = make_points(&[(0, 0), (10, 0), (20, 0), (30, 0)]);
        let deltas = make_points(&[(5, 0), (0, 0), (0, 0), (0, 0)]);
        // The second contour ends before the first one did.
        assert_eq!(
            interpolated_x(&points, &deltas, &[3, 1]).unwrap(),
            vec![5, 15, 25, 35]
        );
    }

    /// A contour end point past the end of the point array is reported rather
    /// than read out of bounds.
    #[test]
    fn contour_end_past_last_point_is_rejected() {
        let points = make_points(&[(0, 0), (10, 0)]);
        let deltas = make_points(&[(5, 0), (0, 0)]);
        assert!(interpolated_x(&points, &deltas, &[9]).is_none());
    }

    // ---- end to end, against a real variable font -------------------------

    /// Vazirmatn has a single `wght` axis. Glyph 1 is a simple glyph with
    /// variation data, glyph 2 is a composite with variation data, and glyph 0
    /// is empty and has none.
    const VAR_GID: GlyphId = GlyphId::new(1);
    const COMPOSITE_GID: GlyphId = GlyphId::new(2);
    const NO_VAR_GID: GlyphId = GlyphId::new(0);

    /// A simple glyph loaded with its point buffer sized to include the
    /// phantom points, as [`Gvar::simple_deltas`] requires.
    struct TestGlyph<'a> {
        gvar: Gvar<'a>,
        points: Vec<Point<i32>>,
        flags: Vec<PointFlags>,
        contours: Vec<u16>,
    }

    impl<'a> TestGlyph<'a> {
        fn new(font: &FontRef<'a>, gid: GlyphId) -> Self {
            let glyf: Glyf<'a> = font.glyf().unwrap();
            let loca: Loca<'a> = font.loca(None).unwrap();
            let Some(Glyph::Simple(simple)) = loca.get_glyf(gid, &glyf).unwrap() else {
                panic!("expected a simple glyph");
            };
            let n = simple.num_points();
            let total = n + PHANTOM_POINT_COUNT;
            let mut points = vec![Point::<i32>::default(); total];
            let mut flags = vec![PointFlags::default(); total];
            simple
                .read_points_fast(&mut points[..n], &mut flags[..n])
                .unwrap();
            let contours = simple
                .end_pts_of_contours()
                .iter()
                .map(|c| c.get())
                .collect();
            Self {
                gvar: font.gvar().unwrap(),
                points,
                flags,
                contours,
            }
        }

        fn deltas(&mut self, gid: GlyphId, coords: &[F2Dot14]) -> (bool, Vec<Point<Fixed>>) {
            let total = self.points.len();
            let mut deltas = vec![Point::<Fixed>::default(); total];
            let mut iup = vec![Point::<Fixed>::default(); total];
            let mut buffers = DeltaBuffers {
                deltas: &mut deltas,
                iup: &mut iup,
            };
            let varied = self
                .gvar
                .simple_deltas(
                    gid,
                    coords,
                    &self.points,
                    &mut self.flags,
                    &self.contours,
                    &mut buffers,
                )
                .unwrap();
            (varied, deltas)
        }
    }

    #[test]
    fn simple_deltas_at_default_location_are_zero() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let mut glyph = TestGlyph::new(&font, VAR_GID);
        // The glyph has variation data, but no tuple is active at the default
        // location, so every delta is zero.
        let (varied, deltas) = glyph.deltas(VAR_GID, &[F2Dot14::from_f32(0.0)]);
        assert!(varied);
        assert!(deltas.iter().all(|d| *d == Point::default()));
    }

    #[test]
    fn simple_deltas_at_extreme_move_points() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let mut glyph = TestGlyph::new(&font, VAR_GID);
        let (varied, deltas) = glyph.deltas(VAR_GID, &[F2Dot14::from_f32(1.0)]);
        assert!(varied);
        let outline_deltas = &deltas[..deltas.len() - PHANTOM_POINT_COUNT];
        assert!(
            outline_deltas.iter().any(|d| *d != Point::default()),
            "expected at least one non-zero outline delta"
        );
    }

    /// Deltas scale with position along the axis: half way along moves points,
    /// and by less than the extreme does.
    #[test]
    fn simple_deltas_scale_along_the_axis() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let mut glyph = TestGlyph::new(&font, VAR_GID);
        let (_, half) = glyph.deltas(VAR_GID, &[F2Dot14::from_f32(0.5)]);
        let (_, full) = glyph.deltas(VAR_GID, &[F2Dot14::from_f32(1.0)]);
        let magnitude = |ds: &[Point<Fixed>]| -> f64 {
            ds.iter()
                .map(|d| d.x.to_f64().abs() + d.y.to_f64().abs())
                .sum()
        };
        let (half_sum, full_sum) = (magnitude(&half), magnitude(&full));
        assert!(half_sum > 0.0);
        assert!(
            half_sum < full_sum,
            "half {half_sum} should be less than full {full_sum}"
        );
    }

    /// Repeated calls must not accumulate: each starts from a zeroed buffer.
    #[test]
    fn deltas_do_not_accumulate_across_calls() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let mut glyph = TestGlyph::new(&font, VAR_GID);
        let (_, once) = glyph.deltas(VAR_GID, &[F2Dot14::from_f32(1.0)]);
        let (_, twice) = glyph.deltas(VAR_GID, &[F2Dot14::from_f32(1.0)]);
        assert_eq!(once, twice);
    }

    /// A glyph with no variation data reports `false` and leaves the buffer
    /// zeroed rather than untouched, so a reused buffer cannot leak deltas from
    /// a previously processed glyph.
    #[test]
    fn simple_deltas_without_variation_data_zeroes_buffer() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let mut glyph = TestGlyph::new(&font, VAR_GID);
        let (varied, deltas) = glyph.deltas(NO_VAR_GID, &[F2Dot14::from_f32(1.0)]);
        assert!(!varied);
        assert!(deltas.iter().all(|d| *d == Point::default()));
    }

    #[test]
    fn simple_deltas_rejects_short_iup_buffer() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let gvar = font.gvar().unwrap();
        let points = [Point::<i32>::default(); 8];
        let mut flags = [PointFlags::default(); 8];
        let mut deltas = [Point::<Fixed>::default(); 8];
        let mut iup = [Point::<Fixed>::default(); 4];
        let mut buffers = DeltaBuffers {
            deltas: &mut deltas,
            iup: &mut iup,
        };
        assert!(matches!(
            gvar.simple_deltas(VAR_GID, &[], &points, &mut flags, &[7], &mut buffers),
            Err(ReadError::InvalidArrayLen)
        ));
    }

    #[test]
    fn simple_deltas_rejects_missing_phantom_points() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let gvar = font.gvar().unwrap();
        // Fewer points than there are phantom points.
        let points = [Point::<i32>::default(); 3];
        let mut flags = [PointFlags::default(); 3];
        let mut deltas = [Point::<Fixed>::default(); 3];
        let mut iup = [Point::<Fixed>::default(); 3];
        let mut buffers = DeltaBuffers {
            deltas: &mut deltas,
            iup: &mut iup,
        };
        assert!(matches!(
            gvar.simple_deltas(VAR_GID, &[], &points, &mut flags, &[2], &mut buffers),
            Err(ReadError::InvalidArrayLen)
        ));
    }

    #[test]
    fn composite_deltas_without_variation_data_zeroes_buffer() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let gvar = font.gvar().unwrap();
        let coords = [F2Dot14::from_f32(0.5)];
        let mut deltas = [Point::new(Fixed::from_i32(7), Fixed::from_i32(9)); 8];
        let varied = gvar
            .composite_deltas(NO_VAR_GID, &coords, &mut deltas)
            .unwrap();
        assert!(!varied);
        assert!(deltas.iter().all(|d| *d == Point::default()));
    }

    #[test]
    fn composite_deltas_with_variation_data_reports_true() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let gvar = font.gvar().unwrap();
        let coords = [F2Dot14::from_f32(1.0)];
        let mut deltas = [Point::<Fixed>::default(); 8];
        let varied = gvar
            .composite_deltas(COMPOSITE_GID, &coords, &mut deltas)
            .unwrap();
        assert!(varied);
    }

    /// Deltas whose position falls past the end of the buffer are ignored
    /// rather than treated as an error, since a composite may legitimately have
    /// fewer components than the variation data describes.
    #[test]
    fn composite_deltas_tolerates_short_buffer() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let gvar = font.gvar().unwrap();
        let coords = [F2Dot14::from_f32(1.0)];
        let mut deltas = [Point::<Fixed>::default(); 1];
        assert!(gvar
            .composite_deltas(COMPOSITE_GID, &coords, &mut deltas)
            .is_ok());
    }
}
