use font_types::BigEndian;

font_types_macro::tables! {
    Maxp05 {
        /// A thing
         version: BigEndian<u16>,
         num_glyphs: BigEndian<u16>,
    }
}

font_types_macro::tables! {
    Maxp10 {
        /// that is
         version: BigEndian<u16>,
         num_glyphs: BigEndian<u16>,
         max_points: BigEndian<u16>,
         max_contours: BigEndian<u16>,
         max_composite_points: BigEndian<u16>,
    }

    #[format(u16)]
    #[generate_getters]
    enum Maxp {
        #[version(1)]
        Version0_5(Maxp05),
        #[version(2)]
        Version1_0(Maxp10),
    }
}

fn main() {}
