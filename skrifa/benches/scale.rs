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
                    let _ = scaler.outline(GlyphId::new(gid), &mut NullPen {}).unwrap();
                }
            })
        });
    }
}

criterion_group!(benches, scale);
criterion_main!(benches);

struct NullPen {}
#[allow(unused_variables)]
impl Pen for NullPen {
    fn move_to(&mut self, x: f32, y: f32) {}
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {}
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {}
    fn line_to(&mut self, x: f32, y: f32) {}
    fn close(&mut self) {}
}
