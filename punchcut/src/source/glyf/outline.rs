use super::Point;
use crate::PathSink;

/// TrueType outline.
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct Outline {
    /// Set of points that define the shape of the outline.
    pub points: Vec<Point>,
    /// Set of tags (one per point).
    pub tags: Vec<u8>,
    /// Index of the end points for each contour in the outline.
    pub contours: Vec<u16>,
    /// True if the scaler applied a scale, in which case the points are in
    /// 26.6 fixed point format. Otherwise, they are in integral font units.
    pub is_scaled: bool,
}

impl Outline {
    /// Creates a new empty outline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties the outline.
    pub fn clear(&mut self) {
        self.points.clear();
        self.tags.clear();
        self.contours.clear();
        self.is_scaled = false;
    }

    /// Converts the outline to a sequence of path commands and invokes the callback for
    /// each on the given sink.
    pub fn to_path(&self, sink: &mut impl PathSink) -> bool {
        #[inline(always)]
        fn conv(p: Point, s: f32) -> (f32, f32) {
            (p.x as f32 * s, p.y as f32 * s)
        }
        const TAG_MASK: u8 = 0x3;
        const QUAD: u8 = 0x0;
        const ON: u8 = 0x1;
        const CUBIC: u8 = 0x2;
        let s = if self.is_scaled { 1. / 64. } else { 1. };
        let points = &self.points;
        let tags = &self.tags;
        let mut count = 0usize;
        let mut last_was_close = false;
        for c in 0..self.contours.len() {
            let mut cur = if c > 0 {
                self.contours[c - 1] as usize + 1
            } else {
                0
            };
            let mut last = self.contours[c] as usize;
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
            if tag == QUAD {
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
                sink.close();
            }
            sink.move_to(p.0, p.1);
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
                        sink.line_to(p.0, p.1);
                        count += 1;
                        last_was_close = false;
                        continue;
                    }
                    QUAD => {
                        let mut do_close_quad = true;
                        let mut v_control = points[cur];
                        while cur < last {
                            cur += 1;
                            let point = points[cur];
                            tag = tags[cur] & TAG_MASK;
                            if tag == ON {
                                let c = conv(v_control, s);
                                let p = conv(point, s);
                                sink.quad_to(c.0, c.1, p.0, p.1);
                                count += 1;
                                last_was_close = false;
                                do_close_quad = false;
                                break;
                            }
                            if tag != QUAD {
                                return false;
                            }
                            let v_middle = Point::new(
                                (v_control.x + point.x) / 2,
                                (v_control.y + point.y) / 2,
                            );
                            let c = conv(v_control, s);
                            let p = conv(v_middle, s);
                            sink.quad_to(c.0, c.1, p.0, p.1);
                            count += 1;
                            last_was_close = false;
                            v_control = point;
                        }
                        if do_close_quad {
                            let c = conv(v_control, s);
                            let p = conv(v_start, s);
                            sink.quad_to(c.0, c.1, p.0, p.1);
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
                            sink.curve_to(c0.0, c0.1, c1.0, c1.1, p.0, p.1);
                            count += 1;
                            last_was_close = false;
                            continue;
                        }
                        let p = conv(v_start, s);
                        sink.curve_to(c0.0, c0.1, c1.0, c1.1, p.0, p.1);
                        count += 1;
                        last_was_close = false;
                        break;
                    }
                }
            }
            if count > 0 && !last_was_close {
                sink.close();
                last_was_close = true;
            }
        }
        true
    }
}
