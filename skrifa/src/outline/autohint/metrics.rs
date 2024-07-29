//! Autohinting specific metrics.

use crate::collections::SmallVec;

// Maximum number of widths, same for Latin and CJK.
//
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L65>
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afcjk.h#L55>
pub const MAX_WIDTHS: usize = 16;

// Maximum number of blue values.
//
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afblue.h#L328>
pub(super) const MAX_BLUES: usize = 8;

// FreeType keeps a single array of blue values per metrics set
// and mutates when the scale factor changes. We'll separate them so
// that we can reuse unscaled metrics as immutable state without
// recomputing them (which is the expensive part).
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L77>
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub(super) struct UnscaledBlue {
    pub position: i32,
    pub overshoot: i32,
    pub ascender: i32,
    pub descender: i32,
    pub flags: u32,
}

pub(super) type UnscaledBlues = SmallVec<UnscaledBlue, MAX_BLUES>;

#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub(crate) struct WidthMetrics {
    /// Used for creating edges.
    pub edge_distance_threshold: i32,
    /// Default stem thickness.
    pub standard_width: i32,
    /// Is standard width very light?
    pub is_extra_light: bool,
}

pub(crate) type UnscaledWidths = SmallVec<i32, MAX_WIDTHS>;

// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afhints.c#L59>
pub(crate) fn sort_and_quantize_widths(widths: &mut UnscaledWidths, threshold: i32) {
    if widths.len() <= 1 {
        return;
    }
    widths.sort_unstable();
    let table = widths.as_mut_slice();
    let mut cur_ix = 0;
    let mut cur_val = table[cur_ix];
    let last_ix = table.len() - 1;
    let mut ix = 1;
    // Compute and use mean values for clusters not larger than
    // `threshold`.
    while ix < table.len() {
        if (table[ix] - cur_val) > threshold || ix == last_ix {
            let mut sum = 0;
            // Fix loop for end of array?
            if (table[ix] - cur_val <= threshold) && ix == last_ix {
                ix += 1;
            }
            for val in &mut table[cur_ix..ix] {
                sum += *val;
                *val = 0;
            }
            table[cur_ix] = sum / ix as i32;
            if ix < last_ix {
                cur_ix = ix + 1;
                cur_val = table[cur_ix];
            }
        }
        ix += 1;
    }
    cur_ix = 1;
    // Compress array to remove zero values
    for ix in 1..table.len() {
        if table[ix] != 0 {
            table[cur_ix] = table[ix];
            cur_ix += 1;
        }
    }
    widths.truncate(cur_ix);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_widths() {
        // We use 10 and 20 as thresholds because the computation used
        // is units_per_em / 100
        assert_eq!(sort_widths_helper(&[1], 10), &[1]);
        assert_eq!(sort_widths_helper(&[1], 20), &[1]);
        assert_eq!(sort_widths_helper(&[60, 20, 40, 35], 10), &[20, 35, 13, 60]);
        assert_eq!(sort_widths_helper(&[60, 20, 40, 35], 20), &[31, 60]);
    }

    fn sort_widths_helper(widths: &[i32], threshold: i32) -> Vec<i32> {
        let mut widths2 = UnscaledWidths::new();
        for width in widths {
            widths2.push(*width);
        }
        sort_and_quantize_widths(&mut widths2, threshold);
        widths2.into_iter().collect()
    }
}
