use ::skrifa::{
    prelude::{LocationRef, Size},
    raw::{
        types::{F2Dot14, Pen},
        FontRef, TableProvider,
    },
    GlyphId, Hinting, MetadataProvider, NativeHinter, OutlineGlyphCollection, ScalerMemory,
};
use skrifa::Tag;

use super::{InstanceOptions, SharedFontData};

pub struct SkrifaInstance<'a> {
    font: FontRef<'a>,
    ppem: f32,
    coords: Vec<F2Dot14>,
    outlines: OutlineGlyphCollection<'a>,
    hinter: Option<NativeHinter>,
}

impl<'a> SkrifaInstance<'a> {
    pub fn new(data: &'a SharedFontData, options: &InstanceOptions) -> Option<Self> {
        let font = FontRef::from_index(data.0.as_ref(), options.index as u32).ok()?;
        let maxp = font.maxp().unwrap();
        if let Some(max_twilight) = maxp.max_twilight_points() {
            let storage_size = maxp.max_storage().unwrap() as usize * 4;
            let stack_size = (maxp.max_stack_elements().unwrap() as usize + 32) * 4;
            let cvt_size = font
                .data_for_tag(Tag::new(b"cvt "))
                .map(|cvt| cvt.as_bytes().len() * 2)
                .unwrap_or(0);
            let twilight_count = max_twilight as usize + 4;
            // 3 copies of points + 1 byte per flag
            let twilight_size = (twilight_count * 3 * 8) + twilight_count;
            // println!(
            //     "hinting size: {}",
            //     storage_size + stack_size + cvt_size + twilight_size
            // );
            // println!("cvt entries: {}", cvt_size / 4);
        }
        let size = if options.ppem != 0 {
            Size::new(options.ppem as f32)
        } else {
            Size::unscaled()
        };
        let outlines = font.outline_glyphs();
        let hinter = if options.ppem != 0 && options.hinting != Hinting::None {
            Some(NativeHinter::new(&outlines, size, options.coords, options.hinting).unwrap())
        } else {
            None
        };
        Some(SkrifaInstance {
            font,
            ppem: size.ppem().unwrap_or_default(),
            coords: options.coords.into(),
            outlines,
            hinter,
        })
    }

    pub fn glyph_count(&self) -> u16 {
        self.font
            .maxp()
            .map(|maxp| maxp.num_glyphs())
            .unwrap_or_default()
    }

    pub fn advance(&mut self, glyph_id: GlyphId) -> Option<f32> {
        self.font
            .glyph_metrics(Size::new(self.ppem), LocationRef::new(&self.coords))
            .advance_width(glyph_id)
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Option<()> {
        let outline = self.outlines.get(glyph_id)?;
        if let Some(hinter) = self.hinter.as_ref() {
            hinter.scale(&outline, ScalerMemory::Auto, pen).unwrap();
        } else {
            outline
                .scale(
                    Size::new(self.ppem),
                    self.coords.as_slice(),
                    ScalerMemory::Auto,
                    pen,
                )
                .ok()?;
        }
        Some(())
    }
}
