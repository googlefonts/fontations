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
pub use metrics::{
    BlueZones, Scale, ScaledAxisMetrics, ScaledBlue, ScaledStyleMetrics, ScaledWidth,
    UnscaledAxisMetrics, UnscaledBlue, UnscaledStyleMetrics, WidthMetrics,
};
pub use outline::Direction;
pub use recorder::{EdgeAction, EdgeHint, HintAction, PointAction, PointHint};
pub use style::{GlyphStyle, ScriptClass, ScriptGroup, StyleClass};
pub use style::{SCRIPT_CLASSES, STYLE_CLASSES};
pub use topo::{Axis, BlueProvenance, Dimension, Edge, Segment, TopoFlags};

use crate::outline::{SmoothMode, Target};
use crate::{FontRef, MetadataProvider};
use alloc::vec::Vec;
use raw::types::{F2Dot14, GlyphId};

/// Plan for hinting an outline.
#[derive(Clone, Debug, Default)]
pub struct HintPlan {
    hints: Vec<HintAction>,
    axes: [Option<Axis>; 2],
}

impl HintPlan {
    /// Creates a new hint plan.
    pub fn new(
        font: &FontRef,
        coords: &[F2Dot14],
        ppem: f32,
        glyph_id: u32,
        glyph_style: GlyphStyle,
    ) -> Option<Self> {
        let style_class = glyph_style.style_class()?;
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
        let hinted_plan = hint::hint_outline_with_recorder(
            &mut outline,
            &metrics,
            &scale,
            Some(glyph_style),
            &mut recorder,
        );

        Some(Self {
            hints: recorder.actions,
            axes: hinted_plan.axes,
        })
    }

    /// Returns the collection of hinting actions.
    pub fn actions(&self) -> &[HintAction] {
        &self.hints
    }

    /// Returns the topological analysis of the horizontal axis.
    pub fn horizontal_axis(&self) -> Option<&Axis> {
        self.axes[Dimension::Horizontal].as_ref()
    }

    // Returns the topological analysis of the vertical axis.
    pub fn vertical_axis(&self) -> Option<&Axis> {
        self.axes[Dimension::Vertical].as_ref()
    }
}

// #[cfg(feature = "autohinter")]
// fn export_segment(segment: topo::Segment) -> ExportedHintSegment {
//     ExportedHintSegment {
//         flags: segment.flags.to_bits(),
//         dir: segment.dir as i8,
//         pos: segment.pos,
//         delta: segment.delta,
//         min_coord: segment.min_coord,
//         max_coord: segment.max_coord,
//         height: segment.height,
//         score: segment.score,
//         len: segment.len,
//         link_ix: segment.link_ix.unwrap_or(u16::MAX),
//         serif_ix: segment.serif_ix.unwrap_or(u16::MAX),
//         first_ix: segment.first_ix,
//         last_ix: segment.last_ix,
//         edge_ix: segment.edge_ix.unwrap_or(u16::MAX),
//         edge_next_ix: segment.edge_next_ix.unwrap_or(u16::MAX),
//     }
// }

// #[cfg(feature = "autohinter")]
// fn export_edge(edge: topo::Edge) -> ExportedHintEdge {
//     let (has_blue, blue_scaled, blue_fitted) = if let Some(blue) = edge.blue_edge {
//         (1, blue.scaled, blue.fitted)
//     } else {
//         (0, 0, 0)
//     };
//     let (blue_ix, blue_is_shoot) = if let Some(blue) = edge.blue_provenance {
//         (blue.index, u8::from(blue.is_shoot))
//     } else {
//         (u16::MAX, 0)
//     };

//     ExportedHintEdge {
//         fpos: edge.fpos,
//         opos: edge.opos,
//         pos: edge.pos,
//         flags: edge.flags.to_bits(),
//         dir: edge.dir as i8,
//         link_ix: edge.link_ix.unwrap_or(u16::MAX),
//         serif_ix: edge.serif_ix.unwrap_or(u16::MAX),
//         scale: edge.scale,
//         first_ix: edge.first_ix,
//         last_ix: edge.last_ix,
//         has_blue,
//         blue_scaled,
//         blue_fitted,
//         blue_ix,
//         blue_is_shoot,
//     }
// }

// #[cfg(feature = "autohinter")]
// fn point_action_code(action: recorder::PointAction) -> u8 {
//     // Values must match the C TA_Action enum's base variant for each family,
//     // as produced by TA_compare_normalize_action() in tabytecode.c,
//     // so that the exported records can be compared directly via memcmp.
//     match action {
//         recorder::PointAction::IpBefore => 0,  // ta_ip_before
//         recorder::PointAction::IpAfter => 1,   // ta_ip_after
//         recorder::PointAction::IpOn => 2,      // ta_ip_on
//         recorder::PointAction::IpBetween => 3, // ta_ip_between
//     }
// }

// #[cfg(feature = "autohinter")]
// fn edge_action_code(action: recorder::EdgeAction) -> u8 {
//     // Values must match the C TA_Action enum's base variant for each family,
//     // as produced by TA_compare_normalize_action() in tabytecode.c,
//     // so that the exported records can be compared directly via memcmp.
//     match action {
//         recorder::EdgeAction::Blue => 4,         // ta_blue
//         recorder::EdgeAction::BlueAnchor => 5,   // ta_blue_anchor
//         recorder::EdgeAction::Anchor => 6,       // ta_anchor
//         recorder::EdgeAction::Adjust => 10,      // ta_adjust
//         recorder::EdgeAction::Link => 22,        // ta_link
//         recorder::EdgeAction::Stem => 26,        // ta_stem
//         recorder::EdgeAction::Serif => 38,       // ta_serif
//         recorder::EdgeAction::SerifAnchor => 45, // ta_serif_anchor
//         recorder::EdgeAction::SerifLink1 => 52,  // ta_serif_link1
//         recorder::EdgeAction::SerifLink2 => 59,  // ta_serif_link2
//         recorder::EdgeAction::Bound => 66,       // ta_bound
//     }
// }

/// All constants are defined based on a UPEM of 2048.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L34>
fn derived_constant(units_per_em: i32, value: i32) -> i32 {
    value * units_per_em / 2048
}
