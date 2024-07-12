//! Computation of blue alignment zones.

use super::super::unscaled::UnscaledOutlineBuf;
use super::script::{blue_flags, ScriptClass};
use crate::{FontRef, MetadataProvider};
use raw::types::F2Dot14;
use raw::TableProvider;

// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afblue.h#L328>
const MAX_BLUES: usize = 8;

#[derive(Copy, Clone, Default, Debug)]
pub(super) struct Width {
    pub org: i32,
    pub cur: i32,
    pub fit: i32,
}

#[derive(Copy, Clone, Default, Debug)]
pub(super) struct Blue {
    pub reference: Width,
    pub overshoot: Width,
    pub ascender: i32,
    pub descender: i32,
    pub flags: u32,
}

impl Blue {
    fn is_latin_any_top(&self) -> bool {
        self.flags & (blue_flags::LATIN_TOP | blue_flags::LATIN_SUB_TOP) != 0
    }
}

#[derive(Clone, Default)]
pub(super) struct Blues {
    blues: [Blue; MAX_BLUES],
    len: usize,
}

impl Blues {
    /// Computes the set of blues for Latin style hinting.
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L314>
    pub fn new_latin(font: &FontRef, coords: &[F2Dot14], script: &ScriptClass) -> Self {
        const MAX_INLINE_POINTS: usize = 64;
        const BLUE_STRING_MAX_LEN: usize = 51;
        let mut blues = Self::default();
        let mut outline_buf = UnscaledOutlineBuf::<MAX_INLINE_POINTS>::new();
        let mut flats = [0; BLUE_STRING_MAX_LEN];
        let mut rounds = [0; BLUE_STRING_MAX_LEN];
        let glyphs = font.outline_glyphs();
        let charmap = font.charmap();
        let units_per_em = font
            .head()
            .map(|head| head.units_per_em())
            .unwrap_or_default() as i32;
        let flat_threshold = units_per_em / 14;
        // Walk over each of the blue character sets for our script.
        for (blue_chars, blue_flags) in script.blues {
            let is_top_like =
                (blue_flags & (blue_flags::LATIN_TOP | blue_flags::LATIN_SUB_TOP)) != 0;
            let is_top = blue_flags & blue_flags::LATIN_TOP != 0;
            let is_x_height = blue_flags & blue_flags::LATIN_X_HEIGHT != 0;
            let is_neutral = blue_flags & blue_flags::LATIN_NEUTRAL != 0;
            let is_long = blue_flags & blue_flags::LATIN_LONG != 0;
            let mut ascender = i16::MIN;
            let mut descender = i16::MAX;
            let mut n_flats = 0;
            let mut n_rounds = 0;
            for ch in *blue_chars {
                // TODO: do some shaping
                let y_offset = 0;
                let Some(gid) = charmap.map(*ch) else {
                    continue;
                };
                if gid.to_u32() == 0 {
                    continue;
                }
                let Some(glyph) = glyphs.get(gid) else {
                    continue;
                };
                outline_buf.clear();
                if glyph.draw_unscaled(coords, None, &mut outline_buf).is_err() {
                    continue;
                }
                let outline = outline_buf.as_ref();
                // Reject glyphs that don't produce any rendering
                if outline.points.len() <= 2 {
                    continue;
                }
                let mut best_y: Option<i16> = None;
                let mut best_y_extremum = if is_top { i32::MIN } else { i32::MAX };
                let mut best_is_round = false;
                // Find the extreme point depending on whether this is a top or bottom blue
                let best_contour_and_point = if is_top_like {
                    outline.find_last_contour(|point| {
                        if best_y.is_none() || Some(point.y) > best_y {
                            best_y = Some(point.y);
                            ascender = ascender.max(point.y + y_offset);
                            true
                        } else {
                            descender = descender.min(point.y + y_offset);
                            false
                        }
                    })
                } else {
                    outline.find_last_contour(|point| {
                        if best_y.is_none() || Some(point.y) < best_y {
                            best_y = Some(point.y);
                            descender = descender.min(point.y + y_offset);
                            true
                        } else {
                            ascender = ascender.max(point.y + y_offset);
                            false
                        }
                    })
                };
                let Some((best_contour_range, best_point_ix)) = best_contour_and_point else {
                    continue;
                };
                let best_contour = &outline.points[best_contour_range];
                // If we have a contour and point then best_y is guaranteed to be Some
                let mut best_y = best_y.unwrap() as i32;
                let best_x = best_contour[best_point_ix].x as i32;
                // Now determine whether the point belongs to a straight or round
                // segment by examining the previous and next points.
                let [mut on_point_first, mut on_point_last] =
                    if best_contour[best_point_ix].is_on_curve() {
                        [Some(best_point_ix); 2]
                    } else {
                        [None; 2]
                    };
                let mut segment_first = best_point_ix;
                let mut segment_last = best_point_ix;
                // Look for the previous and next points on the contour that are not
                // on the same Y coordinate, then threshold the "closeness"
                for (ix, prev) in cycle_backward(best_contour, best_point_ix) {
                    let dist = (prev.y as i32 - best_y).abs();
                    // Allow a small distance or angle (20 == roughly 2.9 degrees)
                    if dist > 5 && ((prev.x as i32 - best_x).abs() <= (20 * dist)) {
                        break;
                    }
                    segment_first = ix;
                    if prev.is_on_curve() {
                        on_point_first = Some(ix);
                        if on_point_last.is_none() {
                            on_point_last = Some(ix);
                        }
                    }
                }
                for (ix, next) in cycle_forward(best_contour, best_point_ix) {
                    let dist = (next.y as i32 - best_y).abs();
                    // Allow a small distance or angle (20 == roughly 2.9 degrees)
                    if dist > 5 && ((next.x as i32 - best_x).abs() <= (20 * dist)) {
                        break;
                    }
                    segment_last = ix;
                    if next.is_on_curve() {
                        on_point_last = Some(ix);
                        if on_point_first.is_none() {
                            on_point_first = Some(ix);
                        }
                    }
                }
                if is_long {
                    // Taken verbatim from FreeType:
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L641>
                    // If this flag is set, we have an additional constraint to
                    // get the blue zone distance: Find a segment of the topmost
                    // (or bottommost) contour that is longer than a heuristic
                    // threshold.  This ensures that small bumps in the outline
                    // are ignored (for example, the `vertical serifs' found in
                    // many Hebrew glyph designs).
                    //
                    // If this segment is long enough, we are done.  Otherwise,
                    // search the segment next to the extremum that is long
                    // enough, has the same direction, and a not too large
                    // vertical distance from the extremum.  Note that the
                    // algorithm doesn't check whether the found segment is
                    // actually the one (vertically) nearest to the extremum.

                    // heuristic threshold value
                    let length_threshold = units_per_em / 25;
                    let dist = outline.points[segment_last].x as i32
                        - outline.points[segment_first].x as i32;
                    if dist < length_threshold
                        && segment_last - segment_first + 2 <= best_contour.len()
                    {
                        // heuristic threshold value
                        let height_threshold = units_per_em / 4;
                        // find previous point with different x value
                        let mut prev_ix = best_point_ix;
                        for (ix, prev) in cycle_backward(best_contour, best_point_ix) {
                            if prev.x as i32 != best_x {
                                prev_ix = ix;
                                break;
                            }
                        }
                        // skip for degenerate case
                        if prev_ix == best_point_ix {
                            continue;
                        }
                        let is_ltr = (outline.points[prev_ix].x as i32) < best_x;
                    }
                }
                best_y += y_offset as i32;
                // Is the segment round?
                // 1. horizontal distance between first and last oncurve point is
                //    larger than a heuristic flat threshold, then it's flat
                // 2. either first or last point of segment is offcurve then it's
                //    round
                let is_round = match (on_point_first, on_point_last) {
                    (Some(first), Some(last))
                        if (best_contour[last].x as i32 - best_contour[first].x as i32).abs()
                            > flat_threshold =>
                    {
                        false
                    }
                    _ => {
                        !best_contour[segment_first].is_on_curve()
                            || !best_contour[segment_last].is_on_curve()
                    }
                };
                if is_round && is_neutral {
                    // Ignore round segments for neutral zone
                    continue;
                }
                // This seems to ignore LATIN_SUB_TOP?
                if is_top {
                    if best_y > best_y_extremum {
                        best_y_extremum = best_y;
                        best_is_round = is_round;
                    }
                } else if best_y < best_y_extremum {
                    best_y_extremum = best_y;
                    best_is_round = is_round;
                }
                if best_y_extremum != i32::MIN && best_y_extremum != i32::MAX {
                    if best_is_round {
                        rounds[n_rounds] = best_y_extremum;
                        n_rounds += 1;
                    } else {
                        flats[n_flats] = best_y_extremum;
                        n_flats += 1;
                    }
                }
            }
            if n_flats == 0 && n_rounds == 0 {
                continue;
            }
            rounds[..n_rounds].sort_unstable();
            flats[..n_flats].sort_unstable();
            let (mut blue_ref, mut blue_shoot) = if n_flats == 0 {
                let val = rounds[n_rounds / 2];
                (val, val)
            } else if n_rounds == 0 {
                let val = flats[n_flats / 2];
                (val, val)
            } else {
                (flats[n_flats / 2], rounds[n_rounds / 2])
            };
            if blue_shoot != blue_ref {
                let over_ref = blue_shoot > blue_ref;
                if is_top_like ^ over_ref {
                    let val = (blue_shoot + blue_ref) / 2;
                    blue_ref = val;
                    blue_shoot = val;
                }
            }
            let mut blue = Blue {
                reference: Width {
                    org: blue_ref,
                    cur: 0,
                    fit: 0,
                },
                overshoot: Width {
                    org: blue_shoot,
                    cur: 0,
                    fit: 0,
                },
                ascender: ascender.into(),
                descender: descender.into(),
                flags: blue_flags
                    & (blue_flags::LATIN_TOP
                        | blue_flags::LATIN_SUB_TOP
                        | blue_flags::LATIN_NEUTRAL),
            };
            if is_x_height {
                blue.flags |= blue_flags::LATIN_BLUE_ADJUSTMENT;
            }
            if let Some(ptr) = blues.blues.get_mut(blues.len) {
                *ptr = blue;
                blues.len += 1;
            }
        }
        if blues.len == 0 {
            return blues;
        }
        // sort bottoms
        let mut sorted_indices: [usize; MAX_BLUES] = core::array::from_fn(|ix| ix);
        let blue_values = blues.values_mut();
        let len = blue_values.len();
        // latin_sort_blues(blue_values, &mut sorted_indices);
        // sort from bottom to top
        for i in 1..blue_values.len() {
            for j in (1..=i).rev() {
                let first = &blue_values[sorted_indices[j - 1]];
                let second = &blue_values[sorted_indices[j]];
                let a = if first.is_latin_any_top() {
                    first.reference.org
                } else {
                    first.overshoot.org
                };
                let b = if second.is_latin_any_top() {
                    second.reference.org
                } else {
                    second.overshoot.org
                };
                if b >= a {
                    break;
                }
                sorted_indices.swap(j, j - 1);
            }
        }
        // and adjust tops
        for i in 0..len - 1 {
            let index1 = sorted_indices[i];
            let index2 = sorted_indices[i + 1];
            let first = &blue_values[index1];
            let second = &blue_values[index2];
            let a = if first.is_latin_any_top() {
                first.overshoot.org
            } else {
                first.reference.org
            };
            let b = if second.is_latin_any_top() {
                second.overshoot.org
            } else {
                second.reference.org
            };
            if a > b {
                if first.is_latin_any_top() {
                    blue_values[index1].overshoot.org = b;
                } else {
                    blue_values[index1].reference.org = b;
                }
            }
        }
        blues
    }

    pub fn values(&self) -> &[Blue] {
        &self.blues[..self.len]
    }

    pub fn values_mut(&mut self) -> &mut [Blue] {
        &mut self.blues[..self.len]
    }
}

impl core::fmt::Debug for Blues {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.values()).finish()
    }
}

fn latin_sort_blues(blues: &[Blue], indices: &mut [usize]) {
    // sort from bottom to top
    for i in 1..blues.len() {
        for j in (1..=i).rev() {
            let first = &blues[indices[j - 1]];
            let second = &blues[indices[j]];
            let a = if first.is_latin_any_top() {
                first.reference.org
            } else {
                first.overshoot.org
            };
            let b = if second.is_latin_any_top() {
                second.reference.org
            } else {
                second.overshoot.org
            };
            if b >= a {
                break;
            }
            indices.swap(j, j - 1);
        }
    }
}

/// Iterator that begins at `start + 1` and cycles through all items
/// of the slice in forward order, ending with `start`.
fn cycle_forward<T>(items: &[T], start: usize) -> impl Iterator<Item = (usize, &T)> {
    let len = items.len();
    let start = start + 1;
    (0..len).map(move |ix| {
        let real_ix = (ix + start) % len;
        (real_ix, &items[real_ix])
    })
}

/// Iterator that begins at `start - 1` and cycles through all items
/// of the slice in reverse order, ending with `start`.
fn cycle_backward<T>(items: &[T], start: usize) -> impl Iterator<Item = (usize, &T)> {
    let len = items.len();
    (0..len).rev().map(move |ix| {
        let real_ix = (ix + start) % len;
        (real_ix, &items[real_ix])
    })
}

#[cfg(test)]
mod tests {
    use raw::FontRef;

    #[test]
    fn cycle_iter_forward() {
        let items = [0, 1, 2, 3, 4, 5, 6, 7];
        let from_5 = super::cycle_forward(&items, 5)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_5, &[6, 7, 0, 1, 2, 3, 4, 5]);
        let from_last = super::cycle_forward(&items, 7)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_last, &items);
    }

    #[test]
    fn cycle_iter_backward() {
        let items = [0, 1, 2, 3, 4, 5, 6, 7];
        let from_5 = super::cycle_backward(&items, 5)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_5, &[4, 3, 2, 1, 0, 7, 6, 5]);
        let from_0 = super::cycle_backward(&items, 0)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_0, &[7, 6, 5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn noto_serif_blues() {
        let font_data = std::fs::read("c:/work/content/fonts/notoserif-regular.ttf").unwrap();
        let font = FontRef::new(&font_data).unwrap();
        let script = &super::super::script::SCRIPT_CLASSES[super::ScriptClass::LATN];
        let blues = super::Blues::new_latin(&font, &[], script);
        let values = blues.values();
        println!("{values:?}");
    }
}
