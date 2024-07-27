//! Latin standard stem width computation.

use super::super::{
    axis::Axis,
    metrics::{self, UnscaledWidths, WidthMetrics, MAX_WIDTHS},
    outline::Outline,
    script::ScriptClass,
};
use crate::MetadataProvider;
use raw::{types::F2Dot14, FontRef, TableProvider};

/// Compute all stem widths and initialize standard width and height for the
/// given script.
///
/// Returns width metrics and unscaled widths for each dimension.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.c#L54>
pub(super) fn compute_widths(
    font: &FontRef,
    coords: &[F2Dot14],
    script: &ScriptClass,
) -> [(WidthMetrics, UnscaledWidths); 2] {
    let mut result: [(WidthMetrics, UnscaledWidths); 2] = Default::default();
    let charmap = font.charmap();
    let glyphs = font.outline_glyphs();
    let units_per_em = font
        .head()
        .map(|head| head.units_per_em() as i32)
        .unwrap_or_default();
    let mut outline = Outline::default();
    let mut axis = Axis::default();
    // We take the first available glyph from the standard character set.
    if let Some(glyph) = script
        .std_chars
        .iter()
        .filter_map(|&ch| glyphs.get(charmap.map(ch)?))
        .next()
    {
        if outline.fill(&glyph, coords).is_ok() {
            // Now process each dimension
            for (dim, (_metrics, widths)) in result.iter_mut().enumerate() {
                axis.reset(dim, outline.orientation);
                super::segments::compute_segments(&mut outline, &mut axis);
                super::segments::link_segments(&outline, &mut axis, 0);
                let segments = axis.segments.as_slice();
                for (segment_ix, segment) in segments.iter().enumerate() {
                    let segment_ix = segment_ix as u16;
                    let Some(link_ix) = segment.link_ix else {
                        continue;
                    };
                    let link = &segments[link_ix as usize];
                    if link_ix > segment_ix && link.link_ix == Some(segment_ix) {
                        let dist = (segment.pos as i32 - link.pos as i32).abs();
                        if widths.len() < MAX_WIDTHS {
                            widths.push(dist);
                        } else {
                            break;
                        }
                    }
                }
                // The value 100 is heuristic
                metrics::sort_and_quantize_widths(widths, units_per_em / 100);
            }
        }
    }
    for (metrics, widths) in result.iter_mut() {
        // Now set derived values
        let stdw = widths
            .first()
            .copied()
            .unwrap_or_else(|| super::derived_constant(units_per_em, 50));
        // Heuristic value of 20% of the smallest width
        metrics.edge_distance_threshold = stdw / 5;
        metrics.standard_width = stdw;
        metrics.is_extra_light = false;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{super::super::script, *};
    use raw::FontRef;

    #[test]
    fn computed_widths() {
        check_widths(
            font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS,
            super::ScriptClass::HEBR,
            [
                (
                    WidthMetrics {
                        edge_distance_threshold: 10,
                        standard_width: 54,
                        is_extra_light: false,
                    },
                    &[54],
                ),
                (
                    WidthMetrics {
                        edge_distance_threshold: 4,
                        standard_width: 21,
                        is_extra_light: false,
                    },
                    &[21, 109],
                ),
            ],
        );
    }

    #[test]
    fn fallback_widths() {
        check_widths(
            font_test_data::CANTARELL_VF_TRIMMED,
            super::ScriptClass::LATN,
            [
                (
                    WidthMetrics {
                        edge_distance_threshold: 4,
                        standard_width: 24,
                        is_extra_light: false,
                    },
                    &[],
                ),
                (
                    WidthMetrics {
                        edge_distance_threshold: 4,
                        standard_width: 24,
                        is_extra_light: false,
                    },
                    &[],
                ),
            ],
        );
    }

    fn check_widths(font_data: &[u8], script_class: usize, expected: [(WidthMetrics, &[i32]); 2]) {
        let font = FontRef::new(font_data).unwrap();
        let script = &script::SCRIPT_CLASSES[script_class];
        let [(hori_metrics, hori_widths), (vert_metrics, vert_widths)] =
            compute_widths(&font, Default::default(), script);
        assert_eq!(hori_metrics, expected[0].0);
        assert_eq!(hori_widths.as_slice(), expected[0].1);
        assert_eq!(vert_metrics, expected[1].0);
        assert_eq!(vert_widths.as_slice(), expected[1].1);
    }
}
