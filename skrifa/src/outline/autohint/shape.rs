//! Shaping support for autohinting.

use raw::{tables::gsub::Gsub, TableProvider};

use crate::{charmap::Charmap, collections::SmallVec, FontRef, GlyphId, MetadataProvider};

/// Determines the fidelity with which we apply shaping in the
/// autohinter.
///
/// Shaping only affects glyph style classification and the glyphs that
/// are chosen for metrics computations. We keep the `Nominal` mode around
/// to enable validation of internal algorithms against a configuration that
/// is known to match FreeType. The `BestEffort` mode should always be
/// used for actual rendering.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum ShaperMode {
    /// Characters are mapped to nominal glyph identifiers and layout tables
    /// are not used for style coverage.
    ///
    /// This matches FreeType when HarfBuzz support is not enabled.
    Nominal,
    /// Simple substitutions are applied according to script rules and layout
    /// tables are used to extend style coverage beyond the character map.
    #[allow(unused)]
    BestEffort,
}

#[derive(Copy, Clone, Default, Debug)]
pub(crate) struct ShapedGlyph {
    pub id: GlyphId,
    /// This may be used for computing vertical alignment zones, particularly
    /// for glyphs like super/subscripts which might have adjustments in GPOS.
    ///
    /// Note that we don't do the same in the horizontal direction which
    /// means that we don't care about the x-offset.
    pub y_offset: i32,
}

/// Arbitrarily chosen to cover our max input size plus some extra to account
/// for expansion from multiple substitution tables.
const SHAPED_CLUSTER_INLINE_SIZE: usize = 16;

/// Container for storing the result of shaping a cluster.
///
/// Some of our input "characters" for metrics computations are actually
/// multi-character [grapheme clusters](https://www.unicode.org/reports/tr29/#Grapheme_Cluster_Boundaries)
/// that may expand to multiple glyphs.
pub(crate) type ShapedCluster = SmallVec<ShapedGlyph, SHAPED_CLUSTER_INLINE_SIZE>;

/// Maps characters to glyphs and handles extended style coverage beyond
/// glyphs that are available in the character map.
///
/// Roughly covers the functionality in <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afshaper.c>.
pub(crate) struct Shaper<'a> {
    font: FontRef<'a>,
    #[allow(unused)]
    mode: ShaperMode,
    charmap: Charmap<'a>,
    #[allow(unused)]
    gsub: Option<Gsub<'a>>,
}

impl<'a> Shaper<'a> {
    pub fn new(font: &FontRef<'a>, mode: ShaperMode) -> Self {
        let charmap = font.charmap();
        let gsub = (mode != ShaperMode::Nominal)
            .then(|| font.gsub().ok())
            .flatten();
        Self {
            font: font.clone(),
            mode,
            charmap,
            gsub,
        }
    }

    pub fn font(&self) -> &FontRef<'a> {
        &self.font
    }

    pub fn charmap(&self) -> &Charmap<'a> {
        &self.charmap
    }

    /// Shapes the given input text with the current mode and stores the
    /// resulting glyphs in the output cluster.
    pub fn shape_cluster(&self, input: &str, output: &mut ShapedCluster) {
        output.clear();
        for (i, ch) in input.chars().enumerate() {
            if i > 0 {
                // In nominal mode, we reject input clusters with multiple
                // characters
                // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/afshaper.c#L639>
                output.clear();
                return;
            }
            output.push(ShapedGlyph {
                id: self.charmap.map(ch).unwrap_or_default(),
                y_offset: 0,
            });
        }
    }
}
