/*!
Representation of a simple outline glyph.
*/

use peniko::kurbo::{
    segments, BezPath, ParamCurveArclen, ParamCurveArea, ParamCurveExtrema, PathEl, Point, Rect,
    Shape,
};

use crate::source::*;

/// Simple outline represented as a sequence of path commands.
#[derive(Clone, Default, Debug)]
pub struct Outline {
    verbs: Vec<Verb>,
    points: Vec<(f32, f32)>,
}

impl Outline {
    /// Creates a new empty outline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new outline from a TrueType outline. Returns `None` if
    /// the source outline is malformed.
    pub fn from_glyf(outline: &glyf::Outline) -> Option<Self> {
        let mut result = Self::new();
        if result.append_glyf(outline) {
            Some(result)
        } else {
            None
        }
    }

    /// Returns true if the outline is empty.
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// Resets the outline to the empty state.
    pub fn clear(&mut self) {
        self.verbs.clear();
        self.points.clear();
    }

    /// Returns an iterator over the path elements of the outline.
    pub fn elements(&self) -> Elements {
        Elements {
            verbs: &self.verbs,
            points: &self.points,
            verb_pos: 0,
            point_pos: 0,
        }
    }

    /// Appends a glyf outline, converting it into a sequence of path elements.
    /// Returns false if the source outline is malformed.
    pub fn append_glyf(&mut self, outline: &glyf::Outline) -> bool {
        #[inline(always)]
        fn conv(p: glyf::Point, s: f32) -> (f32, f32) {
            (p.x as f32 * s, p.y as f32 * s)
        }
        const TAG_MASK: u8 = 0x3;
        const CONIC: u8 = 0x0;
        const ON: u8 = 0x1;
        const CUBIC: u8 = 0x2;
        let s = if outline.is_scaled { 1. / 64. } else { 1. };
        let points = &outline.points;
        let tags = &outline.tags;
        let mut count = 0usize;
        let mut last_was_close = false;
        for c in 0..outline.contours.len() {
            let mut cur = if c > 0 {
                outline.contours[c - 1] as usize + 1
            } else {
                0
            };
            let mut last = outline.contours[c] as usize;
            if last < cur || last >= points.len() {
                return false;
            }
            let mut v_start = points[cur];
            let v_last = points[last];
            let mut tag = tags[cur] & TAG_MASK;
            if tag == CUBIC {
                return false;
            }
            let mut step_point = true;
            if tag == CONIC {
                if tags[last] & TAG_MASK == ON {
                    v_start = v_last;
                    last -= 1;
                } else {
                    v_start.x = (v_start.x + v_last.x) / 2;
                    v_start.y = (v_start.y + v_last.y) / 2;
                }
                step_point = false;
            }
            let p = conv(v_start, s);
            if count > 0 && !last_was_close {
                self.verbs.push(Verb::Close);
            }
            self.verbs.push(Verb::MoveTo);
            self.points.push(p);
            count += 1;
            last_was_close = false;
            while cur < last {
                if step_point {
                    cur += 1;
                }
                step_point = true;
                tag = tags[cur] & TAG_MASK;
                match tag {
                    ON => {
                        let p = conv(points[cur], s);
                        self.verbs.push(Verb::LineTo);
                        self.points.push(p);
                        count += 1;
                        last_was_close = false;
                        continue;
                    }
                    CONIC => {
                        let mut do_close_conic = true;
                        let mut v_control = points[cur];
                        while cur < last {
                            cur += 1;
                            let point = points[cur];
                            tag = tags[cur] & TAG_MASK;
                            if tag == ON {
                                let c = conv(v_control, s);
                                let p = conv(point, s);
                                self.verbs.push(Verb::QuadTo);
                                self.points.extend_from_slice(&[c, p]);
                                count += 1;
                                last_was_close = false;
                                do_close_conic = false;
                                break;
                            }
                            if tag != CONIC {
                                return false;
                            }
                            let v_middle = glyf::Point::new(
                                (v_control.x + point.x) / 2,
                                (v_control.y + point.y) / 2,
                            );
                            let c = conv(v_control, s);
                            let p = conv(v_middle, s);
                            self.verbs.push(Verb::QuadTo);
                            self.points.extend_from_slice(&[c, p]);
                            count += 1;
                            last_was_close = false;
                            v_control = point;
                        }
                        if do_close_conic {
                            let c = conv(v_control, s);
                            let p = conv(v_start, s);
                            self.verbs.push(Verb::QuadTo);
                            self.points.extend_from_slice(&[c, p]);
                            count += 1;
                            last_was_close = false;
                            break;
                        }
                        continue;
                    }
                    _ => {
                        if cur + 1 > last || (tags[cur + 1] & TAG_MASK != CUBIC) {
                            return false;
                        }
                        let c0 = conv(points[cur], s);
                        let c1 = conv(points[cur + 1], s);
                        cur += 2;
                        if cur <= last {
                            let p = conv(points[cur], s);
                            self.verbs.push(Verb::CurveTo);
                            self.points.extend_from_slice(&[c0, c1, p]);
                            count += 1;
                            last_was_close = false;
                            continue;
                        }
                        let p = conv(v_start, s);
                        self.verbs.push(Verb::CurveTo);
                        self.points.extend_from_slice(&[c0, c1, p]);
                        count += 1;
                        last_was_close = false;
                        break;
                    }
                }
            }
            if count > 0 && !last_was_close {
                self.verbs.push(Verb::Close);
                last_was_close = true;
            }
        }
        true
    }
}

impl Shape for Outline {
    type PathElementsIter<'iter> = Elements<'iter>;

    fn path_elements(&self, _tolerance: f64) -> Self::PathElementsIter<'_> {
        self.elements()
    }

    fn to_path(&self, _tolerance: f64) -> BezPath {
        BezPath::from_vec(self.elements().collect())
    }

    fn area(&self) -> f64 {
        segments(self.elements()).map(|seg| seg.signed_area()).sum()
    }

    fn perimeter(&self, accuracy: f64) -> f64 {
        segments(self.elements())
            .map(|seg| seg.arclen(accuracy))
            .sum()
    }

    fn winding(&self, pt: Point) -> i32 {
        segments(self.elements()).map(|seg| seg.winding(pt)).sum()
    }

    fn bounding_box(&self) -> Rect {
        let mut bbox: Option<Rect> = None;
        for seg in segments(self.elements()) {
            let seg_bb = ParamCurveExtrema::bounding_box(&seg);
            if let Some(bb) = bbox {
                bbox = Some(bb.union(seg_bb));
            } else {
                bbox = Some(seg_bb)
            }
        }
        bbox.unwrap_or_default()
    }
}

/// Iterator over the elements of a path.
#[derive(Clone)]
pub struct Elements<'a> {
    verbs: &'a [Verb],
    points: &'a [(f32, f32)],
    verb_pos: usize,
    point_pos: usize,
}

impl<'a> Iterator for Elements<'a> {
    type Item = PathEl;

    fn next(&mut self) -> Option<Self::Item> {
        fn pt(p: (f32, f32)) -> Point {
            Point::new(p.0 as f64, p.1 as f64)
        }
        let verb = self.verbs.get(self.verb_pos)?;
        self.verb_pos += 1;
        Some(match verb {
            Verb::MoveTo => {
                let p0 = self.points[self.point_pos];
                self.point_pos += 1;
                PathEl::MoveTo(pt(p0))
            }
            Verb::LineTo => {
                let p0 = self.points[self.point_pos];
                self.point_pos += 1;
                PathEl::LineTo(pt(p0))
            }
            Verb::QuadTo => {
                let p0 = self.points[self.point_pos];
                let p1 = self.points[self.point_pos + 1];
                self.point_pos += 2;
                PathEl::QuadTo(pt(p0), pt(p1))
            }
            Verb::CurveTo => {
                let p0 = self.points[self.point_pos];
                let p1 = self.points[self.point_pos + 1];
                let p2 = self.points[self.point_pos + 2];
                self.point_pos += 3;
                PathEl::CurveTo(pt(p0), pt(p1), pt(p2))
            }
            Verb::Close => PathEl::ClosePath,
        })
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Verb {
    MoveTo,
    LineTo,
    QuadTo,
    CurveTo,
    Close,
}
