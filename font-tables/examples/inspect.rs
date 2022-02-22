//! Inspect a font, printing information about tables

use font_tables::{tables::TableProvider, FontRef};

fn main() {
    let path = std::env::args().nth(1).expect("missing path argument");
    let bytes = std::fs::read(path).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    print_font_info(&font);
}

fn print_font_info(font: &FontRef) {
    let num_tables = font.table_directory.num_tables();
    println!("loaded {} tables", num_tables);
    for record in font.table_directory.table_records().unwrap() {
        println!(
            "table {} at {:?} (len {})",
            record.tag.get(),
            record.offset.get(),
            record.len.get()
        );
    }

    let head = font.head().expect("missing head");
    println!(
        "\nhead version {}.{}",
        head.major_version, head.minor_version
    );
    println!("revision {}", head.font_revision);
    println!("upm {}", head.units_per_em);
    println!("x/y min: {}, {}", head.x_min, head.y_min);
    println!("x/y max: {}, {}", head.x_max, head.y_max);
}
