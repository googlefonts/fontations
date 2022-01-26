use font_types_macro::font_tables;

font_tables!(
struct TableRecord {
    tag: u32,
    checksum: u32,
    offset: u32,
    length: u32,
});

fn main() {}
