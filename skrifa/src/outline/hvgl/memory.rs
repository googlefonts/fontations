use crate::outline::memory::alloc_slice;

use super::{Outline, Segment, Transform};

/// Buffers used during glyph scaling.
pub(crate) struct HvglOutlineMemory<'a> {
    pub coords: &'a mut [f32],
    pub transforms: &'a mut [Transform],
    pub segments: &'a mut [Segment],
}

impl<'a> HvglOutlineMemory<'a> {
    pub(super) fn new(outline: &Outline, buf: &'a mut [u8]) -> Option<Self> {
        let (coords, buf) = alloc_slice(buf, outline.part.num_total_axes() as usize)?;
        let (transforms, buf) = alloc_slice(buf, outline.part.num_total_subparts() as usize)?;
        let (segments, _) = alloc_slice(buf, outline.max_num_segments as usize)?;
        Some(Self {
            coords,
            transforms,
            segments,
        })
    }
}
