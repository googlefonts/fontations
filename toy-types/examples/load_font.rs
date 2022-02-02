use toy_types::tables::{Cmap4, FontRef, TableProvider};

static TEST_INPUT: &str =
    include_str!("/Users/rofls/dev/projects/xi-mac/test-data/the-golden-boys.txt");

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
    //let upm = head.units_per_em;
    let _32bit_loca = head.index_to_loc_format == 1;
    let maxp = font.maxp().expect("missing maxp");
    //let num_glyphs = maxp.num_glyphs;
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    let cmap = font.cmap().expect("missing cmap");
    eprintln!("cmap ({} tables):", cmap.num_tables);
    let subtable = cmap
        .encoding_records
        .iter()
        .find(|record| cmap.get_subtable_version(record.subtable_offset) == Some(4))
        .and_then(|record| cmap.get_subtable::<Cmap4>(record.subtable_offset))
        .expect("failed to load cmap table");

    for c in ['0', 'a', 'b', 'A', 'l', '.', '*'] {
        let gid = subtable.glyph_id_for_char(c).unwrap_or_default();
        let g_off = loca.get(gid as usize);
        let g_header = g_off.and_then(|off| glyf.get(off as usize));
        eprintln!("'{}': {} {:?}", c, gid, g_header);
    }
    eprintln!("subtable entries: {}", subtable.length);
}
