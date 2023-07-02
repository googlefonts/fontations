use criterion::{criterion_group, criterion_main, Criterion};
use skrifa::{
    prelude::Size,
    raw::{FontRef, TableProvider},
    scale::Pen,
    GlyphId,
};

fn scale(c: &mut Criterion) {
    for (name, font_data) in [
        (
            "Roboto-Regular",
            include_bytes!("./Roboto-Regular.ttf").as_slice(),
        ),
        (
            "SourceSansPro-Regular",
            include_bytes!("SourceSansPro-Regular.otf").as_slice(),
        ),
    ] {
        let font = FontRef::from_index(&font_data, 0).unwrap();
        let glyph_count = font.maxp().unwrap().num_glyphs();
        let mut cx = skrifa::scale::Context::new();
        let mut scaler = cx.new_scaler().size(Size::new(16.0)).build(&font);
        c.bench_function(name, |b| {
            b.iter(|| {
                for gid in 0..glyph_count {
                    let _ = scaler.outline(GlyphId::new(gid), &mut NullPen { i : 0.0 }).unwrap();
                }
            })
        });
    }
}

criterion_group!(benches, scale);
criterion_main!(benches);

struct NullPen { i : f32 }
#[allow(unused_variables)]
impl Pen for NullPen {
    #[inline(never)]
    fn move_to(&mut self, x: f32, y: f32) { self.i += x + y; }
    #[inline(never)]
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) { self.i += cx0 + cy0 + x + y; }
    #[inline(never)]
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) { self.i += cx0 + cy0 + cx1 + cy1 + x + y; }
    #[inline(never)]
    fn line_to(&mut self, x: f32, y: f32) { self.i += x + y; }
    #[inline(never)]
    fn close(&mut self) { self.i += 1.0; }
}
