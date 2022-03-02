//! Inspect a font, printing information about tables

use font_tables::{
    tables::{self, TableProvider},
    FontRef,
};
use font_types::{BigEndian, OffsetHost};

fn main() {
    let path = std::env::args().nth(1).expect("missing path argument");
    let bytes = std::fs::read(path).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    print_font_info(&font);
}

fn print_font_info(font: &FontRef) {
    let num_tables = font.table_directory.num_tables();
    println!("loaded {} tables", num_tables);
    for record in font.table_directory.table_records() {
        println!(
            "table {} at {:?} (len {})",
            record.tag.get(),
            record.offset.get(),
            record.len.get()
        );
    }

    let head = font.head().expect("missing head");
    print_head_info(&head);
    if let Some(hhea) = font.hhea() {
        print_hhea_info(&hhea);
    }
    if let Some(maxp) = font.maxp() {
        print_maxp_info(&maxp);
    }
    if let Some(cmap) = font.cmap() {
        print_cmap_info(&cmap);
    }
}

fn print_head_info(head: &tables::head::Head) {
    println!(
        "\nhead version {}.{}",
        head.major_version, head.minor_version
    );
    println!("  revision {}", head.font_revision);
    println!("  upm {}", head.units_per_em);
    println!("  x/y min: {}, {}", head.x_min, head.y_min);
    println!("  x/y max: {}, {}", head.x_max, head.y_max);
}

fn print_hhea_info(hhea: &tables::hhea::Hhea) {
    println!(
        "\nhhea version {}.{}",
        hhea.major_version(),
        hhea.minor_version()
    );
    println!("  ascender {}", hhea.ascender());
    println!("  descender {}", hhea.descender());
    println!("  line gap {}", hhea.line_gap());
    println!("  max advance {}", hhea.advance_width_max());
    println!("  min left sidebearing {}", hhea.min_left_side_bearing());
    println!("  min right sidebearing {}", hhea.min_right_side_bearing());
}

fn print_maxp_info(maxp: &tables::maxp::Maxp) {
    println!("\nmaxp version {}", maxp.version());
    println!("  num_glyphs: {}", maxp.num_glyphs());
}

fn print_cmap_info(cmap: &tables::cmap::Cmap) {
    println!(
        "\ncmap version {}, {} tables",
        cmap.version(),
        cmap.num_tables()
    );

    for record in cmap.encoding_records() {
        let platform_id = tables::cmap::PlatformId::new(record.platform_id());
        let encoding_id = record.encoding_id();
        let format: BigEndian<u16> = cmap
            .resolve_offset(record.subtable_offset())
            .expect("failed to resolve subtable");
        println!("  ({:?}, {}) format {}", platform_id, encoding_id, format);
    }
}
