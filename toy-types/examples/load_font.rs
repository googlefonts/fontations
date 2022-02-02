#![allow(dead_code, unused_imports)]
use toy_types::tables::{Cmap4, FontRef, TableProvider, TableProviderRef};

fn make_test_chars() -> impl Iterator<Item = char> {
    ('\u{0}'..='\u{FFF}').cycle()
}

fn main() {
    let path = std::env::args().nth(1).expect("missing path argument");
    let bytes = std::fs::read(path).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let cmap = font.cmap().expect("missing cmap");
    let subtable_offset = cmap
        .encoding_records
        .iter()
        .find(|record| cmap.get_subtable_version(record.subtable_offset) == Some(4))
        .map(|record| record.subtable_offset)
        .expect("failed to load cmap table");

    let cmap4 = cmap.get_subtable::<Cmap4>(subtable_offset).unwrap();
    //let cmap4 = cmap.get_zerocopy_cmap4(subtable_offset).unwrap();

    let mut total_area = 0;
    let mut total_chars = 0;
    let mut total_glyphs = 0;

    for c in make_test_chars().take(10_usize.pow(7)) {
        let gid = cmap4.glyph_id_for_char(c);

        // this is artificially bad, we want to exagerate the difference between
        // these two approaches.
        total_chars += 1;
        if let Some(bbox) = get_glyph_bbox2(&font, gid) {
            let width = bbox.x1 - bbox.x0;
            let height = bbox.y1 - bbox.y0;
            total_area += width as usize * (height as usize);
            total_glyphs += 1;
        }
    }
    eprintln!(
        "{} chars\n{} glyphs\n{} area",
        total_chars, total_glyphs, total_area
    );
}

fn print_font_info(font: &FontRef) {
    let num_tables = font.table_directory.num_tables;
    println!("loaded {} tables", num_tables);
    for record in font.table_directory.table_records.iter() {
        println!(
            "table {} at {:?} (len {})",
            std::str::from_utf8(&record.tag).unwrap_or("NULL"),
            record.offset,
            record.len
        );
    }

    let maxp = font.maxp().expect("missing maxp");
    let num_glyphs = maxp.num_glyphs;
    println!("{} glyphs", num_glyphs);
}

struct Bbox {
    x0: i16,
    x1: i16,
    y0: i16,
    y1: i16,
}

/// This version instantiates concrete types
fn get_glyph_bbox1(font: &FontRef, gid: u16) -> Option<Bbox> {
    let head = font.head().expect("missing head");
    let _32bit_loca = head.index_to_loc_format == 1;
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    let g_off = loca.get(gid as usize);
    g_off
        .and_then(|off| glyf.get(off as usize))
        .map(|glyph| Bbox {
            x0: glyph.x_min,
            x1: glyph.x_max,
            y0: glyph.y_min,
            y1: glyph.y_max,
        })
}

/// this version only uses views
fn get_glyph_bbox2(font: &FontRef, gid: u16) -> Option<Bbox> {
    let head = font.head_ref().expect("missing head");
    let _32bit_loca = head.index_to_loc_format()? == 1;
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    let g_off = loca.get(gid as usize);
    g_off
        .and_then(|off| glyf.get_view(off as usize))
        .map(|glyph| Bbox {
            x0: glyph.x_min().unwrap_or(0),
            x1: glyph.x_max().unwrap_or(0),
            y0: glyph.y_min().unwrap_or(0),
            y1: glyph.y_max().unwrap_or(0),
        })
}
