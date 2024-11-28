use skrifa::outline::pen::OutlinePen;

/// Pen that eliminates differences between FreeType and Skrifa.
///
/// This covers three primary cases:
///
/// 1. All contours in FT are implicitly closed while Skrifa emits an
///    explicit close element. This simply drops close elements and replaces
///    them with a line if the current point does not match the most recent
///    start point.
///
/// 2. The FT CFF loader eliminates some, but not all degenerate move/line
///    elements (due to a final scaling step that may introduce new ones).
///    Skrifa applies this pass *after* scaling so is more aggressive about
///    removing degenerates. This drops unused moves and lines that end at the
///    current point.
///
/// 3. The FT TrueType loader in unscaled mode always produces integers. This
///    leads to truncated results when midpoints are computed for implied
///    oncurve points. Skrifa retains the more accurate representation so
///    points are truncated here (in the unscaled case) for comparison.
pub struct RegularizingPen<'a, P> {
    inner: &'a mut P,
    is_scaled: bool,
    pending_move: Option<(f32, f32)>,
    last_start: (f32, f32),
    last_end: Option<(f32, f32)>,
}

impl<'a, P: OutlinePen> RegularizingPen<'a, P> {
    pub fn new(inner: &'a mut P, is_scaled: bool) -> Self {
        Self {
            inner,
            is_scaled,
            pending_move: None,
            last_start: Default::default(),
            last_end: None,
        }
    }

    fn flush_pending_move(&mut self) {
        if let Some(start) = self.pending_move.take() {
            self.inner.move_to(start.0, start.1);
        }
    }

    fn process_coords<const N: usize>(&self, coords: [f32; N]) -> [f32; N] {
        if self.is_scaled {
            coords
        } else {
            coords.map(|x| x.trunc())
        }
    }
}

impl<P: OutlinePen> OutlinePen for RegularizingPen<'_, P> {
    fn move_to(&mut self, x: f32, y: f32) {
        let [x, y] = self.process_coords([x, y]);
        self.pending_move = Some((x, y));
        self.last_start = (x, y);
        self.last_end = Some((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let [x, y] = self.process_coords([x, y]);
        if self.last_end != Some((x, y)) {
            self.flush_pending_move();
            self.inner.line_to(x, y);
            self.last_end = Some((x, y));
        }
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let [cx0, cy0, x, y] = self.process_coords([cx0, cy0, x, y]);
        self.flush_pending_move();
        self.inner.quad_to(cx0, cy0, x, y);
        self.last_end = Some((x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let [cx0, cy0, cx1, cy1, x, y] = self.process_coords([cx0, cy0, cx1, cy1, x, y]);
        self.flush_pending_move();
        self.inner.curve_to(cx0, cy0, cx1, cy1, x, y);
        self.last_end = Some((x, y));
    }

    fn close(&mut self) {
        if self.last_end != Some(self.last_start) {
            self.inner.line_to(self.last_start.0, self.last_start.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skrifa::outline::pen::PathElement;

    #[test]
    fn regularize_case_1_close_commands() {
        use PathElement::*;
        let mut recording = vec![];
        let mut pen = RegularizingPen::new(&mut recording, true);
        // Subpath 1
        pen.move_to(1.0, 2.0);
        pen.line_to(42.0, 24.0);
        pen.close();
        // Subpath 2
        pen.move_to(3.3, 4.4);
        pen.line_to(100.0, 200.0);
        pen.line_to(3.3, 4.4);
        pen.close();
        // Subpath 3 (with curve)
        pen.move_to(3.3, 4.4);
        pen.line_to(100.0, 200.0);
        pen.curve_to(1.0, 2.0, 3.0, 4.0, 3.3, 4.4);
        pen.close();
        assert_eq!(
            recording.as_slice(),
            &[
                // Subpath 1
                MoveTo { x: 1.0, y: 2.0 },
                LineTo { x: 42.0, y: 24.0 },
                // Close is replace with LineTo
                LineTo { x: 1.0, y: 2.0 },
                // Subpath 2
                MoveTo { x: 3.3, y: 4.4 },
                LineTo { x: 100.0, y: 200.0 },
                // Line end point already matches start point, so Close command
                //  is ignored
                LineTo { x: 3.3, y: 4.4 },
                // Subpath 3 (with curve)
                MoveTo { x: 3.3, y: 4.4 },
                LineTo { x: 100.0, y: 200.0 },
                // Curve end point already matches start point, so Close
                // command is ignored
                CurveTo {
                    cx0: 1.0,
                    cy0: 2.0,
                    cx1: 3.0,
                    cy1: 4.0,
                    x: 3.3,
                    y: 4.4
                },
            ]
        );
    }

    #[test]
    fn regularize_case_2_degenerates() {
        use PathElement::*;
        let mut recording = vec![];
        let mut pen = RegularizingPen::new(&mut recording, true);
        // Dropped: superseded by following move
        pen.move_to(1.0, 2.0);
        pen.move_to(4.5, 5.0);
        // Dropped: line to previous move
        pen.line_to(4.5, 5.0);
        pen.curve_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        // Dropped: line to previous curve
        pen.line_to(5.0, 6.0);
        pen.close();
        assert_eq!(
            recording.as_slice(),
            &[
                MoveTo { x: 4.5, y: 5.0 },
                CurveTo {
                    cx0: 1.0,
                    cy0: 2.0,
                    cx1: 3.0,
                    cy1: 4.0,
                    x: 5.0,
                    y: 6.0
                },
                LineTo { x: 4.5, y: 5.0 }
            ]
        );
    }

    #[test]
    fn regularize_case_3_truncate_unscaled() {
        use PathElement::*;
        let mut recording = vec![];
        // Note: false for second parameter denotes unscaled outline
        let mut pen = RegularizingPen::new(&mut recording, false);
        // Simulate computation for offcurve points that generate an implicit
        // oncurve with fractional values:
        // Two offcurve points with odd deltas
        let offcurve1 = (4.0, 4.0);
        let offcurve2 = (7.0, 9.0);
        // Implicit oncurve at midpoint between offcurves
        let implicit_oncurve = (
            (offcurve1.0 + offcurve2.0) / 2.0,
            (offcurve1.1 + offcurve2.1) / 2.0,
        );
        pen.move_to(1.5, 2.5);
        pen.quad_to(
            offcurve1.0,
            offcurve1.1,
            implicit_oncurve.0,
            implicit_oncurve.1,
        );
        pen.quad_to(offcurve2.0, offcurve2.1, 10.0, 12.5);
        // Our implicit oncurve has fractional components
        assert_eq!(implicit_oncurve.0.fract(), 0.5);
        assert_eq!(implicit_oncurve.1.fract(), 0.5);
        assert_eq!(
            recording.as_slice(),
            &[
                MoveTo { x: 1.0, y: 2.0 },
                QuadTo {
                    cx0: offcurve1.0,
                    cy0: offcurve1.1,
                    x: implicit_oncurve.0.trunc(),
                    y: implicit_oncurve.1.trunc()
                },
                QuadTo {
                    cx0: offcurve2.0,
                    cy0: offcurve2.1,
                    x: 10.0,
                    y: 12.0
                },
            ]
        );
    }
}
