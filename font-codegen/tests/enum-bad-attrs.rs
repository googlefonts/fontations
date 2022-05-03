use font_types::{BigEndian, Version16Dot16};

const VERSION_0_5: Version16Dot16 = Version16Dot16::new(0, 5);
const VERSION_1_0: Version16Dot16 = Version16Dot16::new(1, 0);

font_types_macro::tables! {
    Maxp05 {
         version: BigEndian<Version16Dot16>,
         teeth: BigEndian<u16>,
    }

    Maxp10 {
         version: BigEndian<Version16Dot16>,
         num_glyphs: BigEndian<u16>,
    }

    #[format(Version16Dot16)]
    enum Maxp {
        // missing version
        Version0_5(Maxp05),
        #[version(VERSION_1_0)]
        Version1_0(Maxp10),
    }
}

font_types_macro::tables! {
    One {
         one: BigEndian<u16>,
    }

    Two {
         two: BigEndian<u16>,
    }

    // missing version format
    enum OneOrTwo {
        #[version(VERSION_0_5)]
        One(One),
        #[version(VERSION_0_5)]
        Two(Two),
    }
}

font_types_macro::tables! {
    One {
         one: BigEndian<u16>,
    }

    #[format(u16)]
    enum OneOrTwo {
        #[version(MISSING_VERSION)]
        One(One),
    }
}

fn main() {}
