use font_types::{BigEndian, MajorMinor};

font_types_macro::tables! {
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

    #[format(MajorMinor)]
    enum Table {
        #[version(MajorMinor::VERSION_1_0)]
        Version1_0(Table1_0),
        #[version(MajorMinor::VERSION_1_1)]
        Version1_1(Table1_1),
    }
}

fn main() {}
