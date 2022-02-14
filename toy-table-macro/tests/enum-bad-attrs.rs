use raw_types::Version16Dot16;

const VERSION_0_5: Version16Dot16 = Version16Dot16::from_bytes(0x00005000i32.to_be_bytes());
const VERSION_1_0: Version16Dot16 = Version16Dot16::from_bytes(0x00010000i32.to_be_bytes());

toy_table_macro::tables! {
    Maxp05 {
         version: Version16Dot16,
         teeth: Uint16,
    }

    Maxp10 {
         version: Version16Dot16,
         num_glyphs: Uint16,
    }

    #[format(Version16Dot16)]
    enum Maxp {
        // missing version
        Version0_5(Maxp05),
        #[version(VERSION_1_0)]
        Version1_0(Maxp10),
    }
}

toy_table_macro::tables! {
    One {
         one: Uint16,
    }

    Two {
         two: Uint16,
    }

    // missing version format
    enum OneOrTwo {
        #[version(VERSION_0_5)]
        One(One),
        #[version(VERSION_0_5)]
        Two(Two),
    }
}

fn main() {}
