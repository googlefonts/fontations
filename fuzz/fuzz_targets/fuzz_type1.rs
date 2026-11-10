#![no_main]

use libfuzzer_sys::fuzz_target;
use skrifa::{
    font::{glyph::outline::PathSink, Font, Format},
    raw::types::GlyphId,
    DrawMemory, ScalerFont,
};

struct Sink;
impl PathSink for Sink {
    fn move_to(&mut self, _: f32, _: f32) {}
    fn line_to(&mut self, _: f32, _: f32) {}
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
    fn close(&mut self) {}
}

fuzz_target!(|data: &[u8]| {
    if Format::new(data) != Some(Format::Type1) {
        return;
    }
    let Some(font) = Font::new(data.to_vec(), 0) else {
        return;
    };
    let count = font.num_glyphs();
    let scaler = ScalerFont::unhinted(font, None);
    // Bound evaluation work while sampling both ends of sparse CID maps.
    for index in 0..count.min(16) {
        for gid in [index, count - 1 - index] {
            let metrics = scaler.font().glyph_metrics();
            let _ = metrics.h_advance(GlyphId::new(gid));
            let _ = metrics.extents(GlyphId::new(gid));
            if let Some(outline) = scaler.outline(GlyphId::new(gid)) {
                let _ = outline.draw(DrawMemory::Scratch, &mut Sink);
            }
        }
    }
});
