/// [post (PostScript)](https://docs.microsoft.com/en-us/typography/opentype/spec/post#header) table
Post1_0 {
    /// 0x00010000 for version 1.0 0x00020000 for version 2.0
    /// 0x00025000 for version 2.5 (deprecated) 0x00030000 for version
    /// 3.0
    version: BigEndian<Version16Dot16>,
    /// Italic angle in counter-clockwise degrees from the vertical.
    /// Zero for upright text, negative for text that leans to the
    /// right (forward).
    italic_angle: BigEndian<Fixed>,
    /// This is the suggested distance of the top of the underline from
    /// the baseline (negative values indicate below baseline). The
    /// PostScript definition of this FontInfo dictionary key (the y
    /// coordinate of the center of the stroke) is not used for
    /// historical reasons. The value of the PostScript key may be
    /// calculated by subtracting half the underlineThickness from the
    /// value of this field.
    underline_position: BigEndian<FWord>,
    /// Suggested values for the underline thickness. In general, the
    /// underline thickness should match the thickness of the
    /// underscore character (U+005F LOW LINE), and should also match
    /// the strikeout thickness, which is specified in the OS/2 table.
    underline_thickness: BigEndian<FWord>,
    /// Set to 0 if the font is proportionally spaced, non-zero if the
    /// font is not proportionally spaced (i.e. monospaced).
    is_fixed_pitch: BigEndian<u32>,
    /// Minimum memory usage when an OpenType font is downloaded.
    min_mem_type42: BigEndian<u32>,
    /// Maximum memory usage when an OpenType font is downloaded.
    max_mem_type42: BigEndian<u32>,
    /// Minimum memory usage when an OpenType font is downloaded as a
    /// Type 1 font.
    min_mem_type1: BigEndian<u32>,
    /// Maximum memory usage when an OpenType font is downloaded as a
    /// Type 1 font.
    max_mem_type1: BigEndian<u32>,
}

/// [post (PostScript)](https://docs.microsoft.com/en-us/typography/opentype/spec/post#header) table
Post2_0<'a> {
    /// 0x00010000 for version 1.0 0x00020000 for version 2.0
    /// 0x00025000 for version 2.5 (deprecated) 0x00030000 for version
    /// 3.0
    version: BigEndian<Version16Dot16>,
    /// Italic angle in counter-clockwise degrees from the vertical.
    /// Zero for upright text, negative for text that leans to the
    /// right (forward).
    italic_angle: BigEndian<Fixed>,
    /// This is the suggested distance of the top of the underline from
    /// the baseline (negative values indicate below baseline). The
    /// PostScript definition of this FontInfo dictionary key (the y
    /// coordinate of the center of the stroke) is not used for
    /// historical reasons. The value of the PostScript key may be
    /// calculated by subtracting half the underlineThickness from the
    /// value of this field.
    underline_position: BigEndian<FWord>,
    /// Suggested values for the underline thickness. In general, the
    /// underline thickness should match the thickness of the
    /// underscore character (U+005F LOW LINE), and should also match
    /// the strikeout thickness, which is specified in the OS/2 table.
    underline_thickness: BigEndian<FWord>,
    /// Set to 0 if the font is proportionally spaced, non-zero if the
    /// font is not proportionally spaced (i.e. monospaced).
    is_fixed_pitch: BigEndian<u32>,
    /// Minimum memory usage when an OpenType font is downloaded.
    min_mem_type42: BigEndian<u32>,
    /// Maximum memory usage when an OpenType font is downloaded.
    max_mem_type42: BigEndian<u32>,
    /// Minimum memory usage when an OpenType font is downloaded as a
    /// Type 1 font.
    min_mem_type1: BigEndian<u32>,
    /// Maximum memory usage when an OpenType font is downloaded as a
    /// Type 1 font.
    max_mem_type1: BigEndian<u32>,
    /// Number of glyphs (this should be the same as numGlyphs in
    /// 'maxp' table).
    #[hidden]
    num_glyphs: BigEndian<u16>,
    /// Array of indices into the string data. See below for details.
    #[count(num_glyphs)]
    glyph_name_index: [BigEndian<u16>],
    /// Storage for the string data.
    #[count_all]
    string_data: [u8],
}

#[format(Version16Dot16)]
#[generate_getters]
enum Post<'a> {
    #[version(Version16Dot16::VERSION_1_0)]
    Post1_0(Post1_0),
    #[version(Version16Dot16::VERSION_2_0)]
    Post2_0(Post2_0<'a>),
    #[version(Version16Dot16::VERSION_2_5)]
    Post2_5(Post1_0),
    #[version(Version16Dot16::VERSION_3_0)]
    Post3_0(Post1_0),
}
