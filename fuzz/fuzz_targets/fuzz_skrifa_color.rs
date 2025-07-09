#![no_main]
use std::error::Error;

use libfuzzer_sys::fuzz_target;
use skrifa::{
    color::{ColorGlyph, ColorGlyphFormat, ColorPainter},
    instance::{Location, Size},
    FontRef, MetadataProvider,
};

mod helpers;

use helpers::*;

struct NopPainter;

impl ColorPainter for NopPainter {
    fn push_transform(&mut self, _transform: skrifa::Transform) {
        // nop
    }

    fn pop_transform(&mut self) {
        // nop
    }

    fn push_clip_glyph(&mut self, _glyph_id: skrifa::GlyphId) {
        // nop
    }

    fn push_clip_box(&mut self, _clip_box: skrifa::raw::types::BoundingBox<f32>) {
        // nop
    }

    fn pop_clip(&mut self) {
        // nop
    }

    fn fill(&mut self, _brush: skrifa::color::Brush<'_>) {
        // nop
    }

    fn push_layer(&mut self, _composite_mode: skrifa::color::CompositeMode) {
        // nop
    }

    fn pop_layer(&mut self) {
        // nop
    }
}

fn do_color_glyph_things(
    color_glyph: &ColorGlyph,
    size: Size,
    location: &Location,
) -> Result<(), Box<dyn Error>> {
    let _ = color_glyph.bounding_box(location, size);
    let _ = color_glyph.paint(location, &mut NopPainter);
    Ok(())
}

fn do_skrifa_things(font: &FontRef, size: Size, location: &Location) -> Result<(), Box<dyn Error>> {
    let charmap = font.charmap();
    let color_glyphs = font.color_glyphs();

    for glyph_id in charmap.mappings().map(|(_cp, gid)| gid) {
        if let Some(colrv1_glyph) = color_glyphs.get_with_format(glyph_id, ColorGlyphFormat::ColrV1)
        {
            let _ = do_color_glyph_things(&colrv1_glyph, size, location);
        } else if let Some(colrv0_glyph) =
            color_glyphs.get_with_format(glyph_id, ColorGlyphFormat::ColrV0)
        {
            let _ = do_color_glyph_things(&colrv0_glyph, size, location);
        }
    }

    // we don't care about the result, just that we don't panic, hang, etc

    let _ = charmap.has_map();
    let _ = charmap.is_symbol();
    let _ = charmap.has_variant_map();

    let _ = charmap.mappings().count();
    let _ = charmap.variant_mappings().count();

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let Ok(font) = select_font(data) else {
        return;
    };
    // Cross (several sizes) x (default location, random location)
    let scenarios = fuzz_sizes()
        .into_iter()
        .flat_map(|size| {
            fuzz_locations(&font)
                .into_iter()
                .map(move |loc| (size, loc))
        })
        .collect::<Vec<_>>();

    for (size, loc) in scenarios {
        let _ = do_skrifa_things(&font, size, &loc);
    }
});
