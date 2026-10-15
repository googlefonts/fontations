//! Small COLR paint graphs for reader, writer, traversal and subset tests.

use crate::bebuffer::BeBuffer;

/// PaintGlyph2 for outline glyph 0x123456, filled with palette entry 7.
pub const PAINT_GLYPH2: &[u8] = &[
    33, 0, 0, 7, 0x12, 0x34, 0x56, // PaintGlyph2: child at offset 7
    2, 0, 7, 0x40, 0, // PaintSolid: palette 7, alpha 1
];

/// COLRv1 base glyph 42 referencing [`PAINT_GLYPH2`].
pub fn paint_glyph2_colr() -> BeBuffer {
    BeBuffer::new()
        .push(1u16) // version
        .push(0u16) // numBaseGlyphRecords
        .push(0u32) // baseGlyphRecordsOffset
        .push(0u32) // layerRecordsOffset
        .push(0u16) // numLayerRecords
        .push(34u32) // baseGlyphListOffset
        .push(0u32) // layerListOffset
        .push(0u32) // clipListOffset
        .push(0u32) // varIndexMapOffset
        .push(0u32) // itemVariationStoreOffset
        .push(1u32) // numBaseGlyphPaintRecords
        .push(42u16) // glyphID
        .push(10u32) // paintOffset, relative to BaseGlyphList
        .extend(PAINT_GLYPH2.iter().copied())
}
