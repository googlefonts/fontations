use font_types::{BigEndian, Tag, Version16Dot16};

const VERSION_0_5: Version16Dot16 = Version16Dot16::new(0, 5);
const VERSION_1_0: Version16Dot16 = Version16Dot16::new(1, 0);

pub const TAG: Tag = Tag::new(b"maxp");

font_types::tables! {
    /// [`maxp`](https://docs.microsoft.com/en-us/typography/opentype/spec/maxp)
    Maxp0_5 {
        /// 0x00005000 for version 0.5
        version: BigEndian<Version16Dot16>,
        /// The number of glyphs in the font.
        num_glyphs: BigEndian<u16>,
    }

    /// [`maxp`](https://docs.microsoft.com/en-us/typography/opentype/spec/maxp)
    Maxp1_0 {
        /// 0x00010000 for version 1.0.
        version: BigEndian<Version16Dot16>,
        /// The number of glyphs in the font.
        num_glyphs: BigEndian<u16>,
        /// Maximum points in a non-composite glyph.
        max_points: BigEndian<u16>,
        /// Maximum contours in a non-composite glyph.
        max_contours: BigEndian<u16>,
        /// Maximum points in a composite glyph.
        max_composite_points: BigEndian<u16>,
        /// Maximum contours in a composite glyph.
        max_composite_contours: BigEndian<u16>,
        /// 1 if instructions do not use the twilight zone (Z0), or 2 if
        /// instructions do use Z0; should be set to 2 in most cases.
        max_zones: BigEndian<u16>,
        /// Maximum points used in Z0.
        max_twilight_points: BigEndian<u16>,
        /// Number of Storage Area locations.
        max_storage: BigEndian<u16>,
        /// Number of FDEFs, equal to the highest function number + 1.
        max_function_defs: BigEndian<u16>,
        /// Number of IDEFs.
        max_instruction_defs: BigEndian<u16>,
        /// Maximum stack depth across Font Program ('fpgm' table), CVT
        /// Program ('prep' table) and all glyph instructions (in the
        /// 'glyf' table).
        max_stack_elements: BigEndian<u16>,
        /// Maximum byte count for glyph instructions.
        max_size_of_instructions: BigEndian<u16>,
        /// Maximum number of components referenced at “top level” for
        /// any composite glyph.
        max_component_elements: BigEndian<u16>,
        /// Maximum levels of recursion; 1 for simple components.
        max_component_depth: BigEndian<u16>,
    }

    #[format(Version16Dot16)]
    #[generate_getters]
    enum Maxp {
        #[version(VERSION_0_5)]
        Version0_5(Maxp0_5),
        #[version(VERSION_1_0)]
        Version1_0(Maxp1_0),
    }
}
