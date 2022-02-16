use raw_types::Uint16;

const ONE: Uint16 = Uint16::from_bytes(1u16.to_be_bytes());

type MajorMinorU16 = [Uint16; 2];

const VERSION_1_0: MajorMinorU16 = [ONE, Uint16::ZERO];
const VERSION_1_1: MajorMinorU16 = [ONE, ONE];

toy_table_macro::tables! {
    Table1_0 {
         version_major: Uint16,
         version_minor: Uint16,
         num_glyphs: Uint16,
    }

    Table1_1 {
         version_major: Uint16,
         version_minor: Uint16,
         num_glyphs: Uint16,
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
