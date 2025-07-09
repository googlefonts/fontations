use raw::{tables::hvgl::Part, types::GlyphId};

use crate::Transform;

/// Represents the information necessary to scale a glyph outline.
///
/// Contains a reference to the glyph data itself as well as metrics that
/// can be used to compute the memory requirements for scaling the glyph.
#[derive(Clone)]
pub struct Outline<'a> {
    pub glyph_id: GlyphId,
    /// The associated top-level glyph part for the outline.
    pub(super) part: Part<'a>,
    /// Maximum segment count for any shape part that's part of this outline.
    /// Each segment contains an on-curve and off-curve point.
    pub(super) max_num_segments: u16,
}

impl Outline<'_> {
    /// Returns the minimum size in bytes required to scale an outline based
    /// on the computed sizes.
    pub fn required_buffer_size(&self) -> usize {
        let mut size = 0;
        // Each segment is made up of 4 f64s (2 each for the on-curve and
        // off-curve points). We want to allocate an array so we can apply any
        // deltas to each segment one axis at a time, rather than one segment at
        // a time, as the latter is much slower due to pipeline stalls.
        size += self.max_num_segments as usize * size_of::<f64>() * 4;
        // One transform per subpart
        size += self.part.num_total_subparts() as usize * size_of::<Transform>();
        // One axis coordinate per axis
        size += self.part.num_total_axes() as usize * size_of::<f32>();
        if size != 0 {
            // If we're given a buffer that is not aligned, we'll need to
            // adjust, so add our maximum alignment requirement in bytes.
            size += std::mem::align_of::<f64>();
        }
        size
    }
}
