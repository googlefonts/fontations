//! Runtime autohinting support.

mod hint;
mod instance;
mod metrics;
mod outline;
mod recorder;
mod shape;
mod style;
mod topo;

pub use instance::GlyphStyles;
pub(crate) use instance::Instance;
#[cfg(feature = "autohinter")]
pub use metrics::{
    compute_scaled_style_metrics_exported, compute_unscaled_style_metrics_exported,
    ExportedScaledBlue, ExportedScaledStyleMetrics, ExportedScaledWidth, ExportedUnscaledBlue,
    ExportedUnscaledStyleMetrics,
};

#[cfg(feature = "autohinter")]
pub use style::{SCRIPT_CLASSES, STYLE_CLASSES};

#[cfg(feature = "autohinter")]
use crate::outline::{SmoothMode, Target};
#[cfg(feature = "autohinter")]
use crate::{FontRef, MetadataProvider};
#[cfg(feature = "autohinter")]
use alloc::vec::Vec;
#[cfg(feature = "autohinter")]
use raw::types::{F2Dot14, GlyphId};

#[cfg(feature = "autohinter")]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ExportedHintRecord {
    pub action: u8,
    pub dim: u8,
    pub point_ix: u16,
    pub edge_ix: u16,
    pub edge2_ix: u16,
    pub edge3_ix: u16,
    pub lower_bound_ix: u16,
    pub upper_bound_ix: u16,
}

#[cfg(feature = "autohinter")]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ExportedHintSegment {
    pub flags: u8,
    pub dir: i8,
    pub pos: i16,
    pub delta: i16,
    pub min_coord: i16,
    pub max_coord: i16,
    pub height: i16,
    pub score: i32,
    pub len: i32,
    pub link_ix: u16,
    pub serif_ix: u16,
    pub first_ix: u16,
    pub last_ix: u16,
    pub edge_ix: u16,
    pub edge_next_ix: u16,
}

#[cfg(feature = "autohinter")]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ExportedHintEdge {
    pub fpos: i16,
    pub opos: i32,
    pub pos: i32,
    pub flags: u8,
    pub dir: i8,
    pub link_ix: u16,
    pub serif_ix: u16,
    pub scale: i32,
    pub first_ix: u16,
    pub last_ix: u16,
    pub has_blue: u8,
    pub blue_scaled: i32,
    pub blue_fitted: i32,
    pub blue_ix: u16,
    pub blue_is_shoot: u8,
}

#[cfg(feature = "autohinter")]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ExportedHintPlan {
    pub records: Vec<ExportedHintRecord>,
    pub segments: Vec<ExportedHintSegment>,
    pub edges: Vec<ExportedHintEdge>,
}

#[cfg(feature = "autohinter")]
pub fn compute_hint_plan_exported(
    font: &FontRef,
    coords: &[F2Dot14],
    glyph_id: u32,
    style_index: usize,
    is_non_base: bool,
    is_digit: bool,
    ppem: f32,
) -> Option<ExportedHintPlan> {
    let style_class = STYLE_CLASSES.get(style_index)?;
    let outline_glyph = font.outline_glyphs().get(GlyphId::new(glyph_id))?;

    let shaper_mode = if cfg!(feature = "autohint_shaping") {
        shape::ShaperMode::BestEffort
    } else {
        shape::ShaperMode::Nominal
    };
    let shaper = shape::Shaper::new(font, shaper_mode);
    let metrics = metrics::compute_unscaled_style_metrics(&shaper, coords, style_class);

    let mut outline = outline::Outline::default();
    outline.fill(&outline_glyph, coords).ok()?;

    let glyph_style = style::GlyphStyle::from_parts(style_index as u16, is_non_base, is_digit);
    let scale = metrics::Scale::new(
        ppem,
        outline_glyph.units_per_em() as i32,
        font.attributes().style,
        Target::Smooth {
            mode: SmoothMode::Normal,
            symmetric_rendering: true,
            preserve_linear_metrics: false,
        },
        metrics.style_class().script.group,
    );

    let mut recorder = recorder::HintsRecorder::default();
    let hinted_plan = hint::hint_outline_with_plan(
        &mut outline,
        &metrics,
        &scale,
        Some(glyph_style),
        Some(&mut recorder),
    );

    let vertical_axis = hinted_plan.vertical_axis;
    let records = export_records(recorder.records);
    let (segments, edges) = if let Some(axis) = vertical_axis {
        let segments = axis.segments.iter().copied().map(export_segment).collect();
        let edges = axis.edges.iter().copied().map(export_edge).collect();
        (segments, edges)
    } else {
        (Vec::new(), Vec::new())
    };

    Some(ExportedHintPlan {
        records,
        segments,
        edges,
    })
}

#[cfg(feature = "autohinter")]
pub fn compute_hint_records_exported(
    font: &FontRef,
    coords: &[F2Dot14],
    glyph_id: u32,
    style_index: usize,
    is_non_base: bool,
    is_digit: bool,
    ppem: f32,
) -> Option<Vec<ExportedHintRecord>> {
    Some(
        compute_hint_plan_exported(
            font,
            coords,
            glyph_id,
            style_index,
            is_non_base,
            is_digit,
            ppem,
        )?
        .records,
    )
}

#[cfg(feature = "autohinter")]
fn export_records(records: Vec<recorder::HintRecord>) -> Vec<ExportedHintRecord> {
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        match record {
            recorder::HintRecord::Point(point) => {
                // Skip horizontal hints: C's TA_compare_record_hint filters
                // out TA_DIMENSION_HORZ (= 0) and only records vertical hints.
                if point.dim == topo::Axis::HORIZONTAL {
                    continue;
                }
                out.push(ExportedHintRecord {
                    action: action_code(point.action),
                    dim: point.dim as u8,
                    point_ix: point.point_ix,
                    edge_ix: point.edge_ix.unwrap_or(u16::MAX),
                    edge2_ix: point.edge2_ix.unwrap_or(u16::MAX),
                    edge3_ix: u16::MAX,
                    lower_bound_ix: u16::MAX,
                    upper_bound_ix: u16::MAX,
                });
            }
            recorder::HintRecord::Edge(edge) => {
                // Skip horizontal hints (same reason as above).
                if edge.dim == topo::Axis::HORIZONTAL {
                    continue;
                }
                out.push(ExportedHintRecord {
                    action: action_code(edge.action),
                    dim: edge.dim as u8,
                    point_ix: u16::MAX,
                    edge_ix: edge.edge_ix,
                    edge2_ix: edge.edge2_ix.unwrap_or(u16::MAX),
                    edge3_ix: edge.edge3_ix.unwrap_or(u16::MAX),
                    lower_bound_ix: edge.lower_bound_ix.unwrap_or(u16::MAX),
                    upper_bound_ix: edge.upper_bound_ix.unwrap_or(u16::MAX),
                });
            }
        }
    }

    out
}

#[cfg(feature = "autohinter")]
fn export_segment(segment: topo::Segment) -> ExportedHintSegment {
    ExportedHintSegment {
        flags: segment.flags,
        dir: segment.dir as i8,
        pos: segment.pos,
        delta: segment.delta,
        min_coord: segment.min_coord,
        max_coord: segment.max_coord,
        height: segment.height,
        score: segment.score,
        len: segment.len,
        link_ix: segment.link_ix.unwrap_or(u16::MAX),
        serif_ix: segment.serif_ix.unwrap_or(u16::MAX),
        first_ix: segment.first_ix,
        last_ix: segment.last_ix,
        edge_ix: segment.edge_ix.unwrap_or(u16::MAX),
        edge_next_ix: segment.edge_next_ix.unwrap_or(u16::MAX),
    }
}

#[cfg(feature = "autohinter")]
fn export_edge(edge: topo::Edge) -> ExportedHintEdge {
    let (has_blue, blue_scaled, blue_fitted) = if let Some(blue) = edge.blue_edge {
        (1, blue.scaled, blue.fitted)
    } else {
        (0, 0, 0)
    };
    let (blue_ix, blue_is_shoot) = if let Some(blue) = edge.blue_provenance {
        (blue.blue_ix, u8::from(blue.is_shoot))
    } else {
        (u16::MAX, 0)
    };

    ExportedHintEdge {
        fpos: edge.fpos,
        opos: edge.opos,
        pos: edge.pos,
        flags: edge.flags,
        dir: edge.dir as i8,
        link_ix: edge.link_ix.unwrap_or(u16::MAX),
        serif_ix: edge.serif_ix.unwrap_or(u16::MAX),
        scale: edge.scale,
        first_ix: edge.first_ix,
        last_ix: edge.last_ix,
        has_blue,
        blue_scaled,
        blue_fitted,
        blue_ix,
        blue_is_shoot,
    }
}

#[cfg(feature = "autohinter")]
fn action_code(action: recorder::Action) -> u8 {
    // Values must match the C TA_Action enum's base variant for each family,
    // as produced by TA_compare_normalize_action() in tabytecode.c,
    // so that the exported records can be compared directly via memcmp.
    match action {
        recorder::Action::IpBefore => 0,     // ta_ip_before
        recorder::Action::IpAfter => 1,      // ta_ip_after
        recorder::Action::IpOn => 2,         // ta_ip_on
        recorder::Action::IpBetween => 3,    // ta_ip_between
        recorder::Action::Blue => 4,         // ta_blue
        recorder::Action::BlueAnchor => 5,   // ta_blue_anchor
        recorder::Action::Anchor => 6,       // ta_anchor
        recorder::Action::Adjust => 10,      // ta_adjust
        recorder::Action::Link => 22,        // ta_link
        recorder::Action::Stem => 26,        // ta_stem
        recorder::Action::Serif => 38,       // ta_serif
        recorder::Action::SerifAnchor => 45, // ta_serif_anchor
        recorder::Action::SerifLink1 => 52,  // ta_serif_link1
        recorder::Action::SerifLink2 => 59,  // ta_serif_link2
        recorder::Action::Bound => 66,       // ta_bound
    }
}

/// All constants are defined based on a UPEM of 2048.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L34>
fn derived_constant(units_per_em: i32, value: i32) -> i32 {
    value * units_per_em / 2048
}
