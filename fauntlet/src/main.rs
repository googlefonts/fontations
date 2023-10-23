use std::path::Path;

use rayon::prelude::*;

use fauntlet::{FontInstance, FreeTypeFont, RecordingPen, RegularizingPen, SkrifaFont};
use skrifa::{raw::types::F2Dot14, GlyphId};

fn main() {
    // Pixels per em sizes
    let ppem_sizes = [8, 16, 50, 72, 113, 144];

    // Locations in normalized variation space
    let var_locations = [-1.0, -0.32, 0.0, 0.42, 1.0].map(|c| F2Dot14::from_f32(c));

    let args = std::env::args().collect::<Vec<_>>();

    let Some(font_paths) = args.get(1) else {
        println!("Missing path to font file(s).");
        return;
    };

    let font_paths = glob::glob(font_paths)
        .unwrap()
        .filter_map(|x| x.ok())
        .collect::<Vec<_>>();

    font_paths.par_iter().for_each(|font_path| {
        // println!("[{font_path:?}]");
        if let Some(mut font_data) = fauntlet::FontFileData::new(&font_path) {
            for font_ix in 0..font_data.count() {
                for ppem in &ppem_sizes {
                    let axis_count = font_data.axis_count(font_ix) as usize;
                    if axis_count != 0 {
                        let mut coords = vec![];
                        for coord in &var_locations {
                            coords.clear();
                            coords.extend(std::iter::repeat(*coord).take(axis_count));
                            let instance = fauntlet::FontInstance::new(font_ix, *ppem, &coords);
                            if let Some(fonts) = font_data.get(&instance) {
                                compare_outlines(&font_path, &instance, fonts);
                            }
                        }
                    } else {
                        let instance = FontInstance::new(font_ix, *ppem, &[]);
                        if let Some(fonts) = font_data.get(&instance) {
                            compare_outlines(&font_path, &instance, fonts);
                        }
                    }
                }
            }
        }
    });
}

fn compare_outlines(
    path: &Path,
    instance: &FontInstance,
    (mut ft_font, mut skrifa_font): (FreeTypeFont, SkrifaFont),
) {
    let glyph_count = skrifa_font.glyph_count();

    for gid in 0..glyph_count {
        let gid = GlyphId::new(gid);

        let mut ft_outline = RecordingPen::default();
        ft_font
            .outline(gid, &mut RegularizingPen::new(&mut ft_outline))
            .unwrap();

        let mut skrifa_outline = RecordingPen::default();
        skrifa_font
            .outline(gid, &mut RegularizingPen::new(&mut skrifa_outline))
            .unwrap();

        if ft_outline != skrifa_outline {
            println!(
                "[{path:?}#{} ppem: {} coords: {:?}] glyph id {} doesn't match:\nFreeType: {:?}\nSkrifa:   {:?}",
                instance.index,
                instance.ppem,
                instance.coords,
                gid.to_u16(),
                &ft_outline.0,
                &skrifa_outline.0,
            );
            std::process::abort();
        }
    }
}
