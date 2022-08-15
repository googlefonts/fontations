#![parse_module(font_tables::tables::maxp)]

/// [`maxp`](https://docs.microsoft.com/en-us/typography/opentype/spec/maxp)
table Maxp {
    /// The version: 0x00005000 for version 0.5, 0x00010000 for version 1.0.
    #[version]
    version: BigEndian<Version16Dot16>,
    /// The number of glyphs in the font.
    num_glyphs: BigEndian<u16>,
    /// Maximum points in a non-composite glyph.
    #[available(Version16Dot16::VERSION_1_0)]
    max_points: BigEndian<u16>,
    /// Maximum contours in a non-composite glyph.
    #[available(Version16Dot16::VERSION_1_0)]
    max_contours: BigEndian<u16>,
    /// Maximum points in a composite glyph.
    #[available(Version16Dot16::VERSION_1_0)]
    max_composite_points: BigEndian<u16>,
    /// Maximum contours in a composite glyph.
    #[available(Version16Dot16::VERSION_1_0)]
    max_composite_contours: BigEndian<u16>,
    /// 1 if instructions do not use the twilight zone (Z0), or 2 if
    /// instructions do use Z0; should be set to 2 in most cases.
    #[available(Version16Dot16::VERSION_1_0)]
    max_zones: BigEndian<u16>,
    /// Maximum points used in Z0.
    #[available(Version16Dot16::VERSION_1_0)]
    max_twilight_points: BigEndian<u16>,
    /// Number of Storage Area locations.
    #[available(Version16Dot16::VERSION_1_0)]
    max_storage: BigEndian<u16>,
    /// Number of FDEFs, equal to the highest function number + 1.
    #[available(Version16Dot16::VERSION_1_0)]
    max_function_defs: BigEndian<u16>,
    /// Number of IDEFs.
    #[available(Version16Dot16::VERSION_1_0)]
    max_instruction_defs: BigEndian<u16>,
    /// Maximum stack depth across Font Program ('fpgm' table), CVT
    /// Program ('prep' table) and all glyph instructions (in the
    /// 'glyf' table).
    #[available(Version16Dot16::VERSION_1_0)]
    max_stack_elements: BigEndian<u16>,
    /// Maximum byte count for glyph instructions.
    #[available(Version16Dot16::VERSION_1_0)]
    max_size_of_instructions: BigEndian<u16>,
    /// Maximum number of components referenced at “top level” for
    /// any composite glyph.
    #[available(Version16Dot16::VERSION_1_0)]
    max_component_elements: BigEndian<u16>,
    /// Maximum levels of recursion; 1 for simple components.
    #[available(Version16Dot16::VERSION_1_0)]
    max_component_depth: BigEndian<u16>,
}
