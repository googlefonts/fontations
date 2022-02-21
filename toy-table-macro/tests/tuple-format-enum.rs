use font_types::BigEndian;

type MajorMinorU16 = [u16; 2];

const VERSION_1_0: MajorMinorU16 = [1, 0];
const VERSION_1_1: MajorMinorU16 = [1, 1];

toy_table_macro::tables! {
    Table1_0 {
         version_major: BigEndian<u16>,
         version_minor: BigEndian<u16>,
         num_glyphs: BigEndian<u16>,
    }

    Table1_1 {
         version_major: BigEndian<u16>,
         version_minor: BigEndian<u16>,
         num_glyphs: BigEndian<u16>,
    }

    #[format(MajorMinorU16)]
    enum Table {
        #[version(VERSION_1_0)]
        Version1_0(Table1_0),
        #[version(VERSION_1_1)]
        Version1_1(Table1_1),
    }
}

fn main() {}
