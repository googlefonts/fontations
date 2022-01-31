use toy_types::tables::{FontRef, TableProvider};

fn main() {
    let path = std::env::args().nth(1).expect("missing path argument");
    let bytes = std::fs::read(path).unwrap();
    let font = FontRef::new(&bytes).unwrap();
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

    let head = font.head().expect("missing head");
    let upm = head.units_per_em;
    let _32bit_loca = head.index_to_loc_format == 1;
    dbg!(head);
    let maxp = font.maxp().expect("missing maxp");
    let num_glyphs = maxp.num_glyphs;
    dbg!(maxp);
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    for (i, offset) in loca.iter().take(10).enumerate() {
        let glyph_header = glyf.get(offset as usize).expect("missing glyf table");
        eprintln!("{} off {}: {:?}", i, offset, glyph_header);
    }
    //offset = loca.get(i);
}
