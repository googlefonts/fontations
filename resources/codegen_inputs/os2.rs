#![parse_module(read_fonts::tables::os2)]

/// [`os2`](https://docs.microsoft.com/en-us/typography/opentype/spec/os2)
table Os2 {
    /// The version: 0x00005000 for version 0.5, 0x00010000 for version 1.0.
    #[version]
    #[compile(self.compute_version())]
    version: BigEndian<Version16Dot16>,
    /// The Average Character Width parameter specifies the arithmetic average
    /// of the escapement (width) of all non-zero width glyphs in the font.
    x_avg_char_width: BigEndian<i16>,
    /// Indicates the visual weight (degree of blackness or thickness of
    /// strokes) of the characters in the font. Values from 1 to 1000 are valid.
    us_weight_class: BigEndian<u16>,
    /// Indicates a relative change from the normal aspect ratio (width to
    /// height ratio) as specified by a font designer for the glyphs in a font.
    us_width_class: BigEndian<u16>,
    /// Indicates font embedding licensing rights for the font.
    fs_type: BigEndian<u16>,
    /// The recommended horizontal size in font design units for subscripts for
    /// this font.
    y_subscript_x_size: BigEndian<i16>,
    /// The recommended vertical size in font design units for subscripts for
    /// this font.
    y_subscript_y_size: BigEndian<i16>,
    /// The recommended horizontal offset in font design units for subscripts
    /// for this font.
    y_subscript_x_offset: BigEndian<i16>,
    /// The recommended vertical offset in font design units for subscripts
    /// for this font.
    y_subscript_y_offset: BigEndian<i16>,
    /// The recommended horizontal size in font design units for superscripts
    /// for this font.
    y_superscript_x_size: BigEndian<i16>,
    /// The recommended vertical size in font design units for superscripts
    /// for this font.
    y_superscript_y_size: BigEndian<i16>,
    /// The recommended horizontal offset in font design units for superscripts
    /// for this font.
    y_superscript_x_offset: BigEndian<i16>,
    /// The recommended vertical offset in font design units for superscripts
    /// for this font.
    y_superscript_y_offset: BigEndian<i16>,
    /// Thickness of the strikeout stroke in font design units.
    y_strikeout_size: BigEndian<i16>,
    /// The position of the top of the strikeout stroke relative to the
    /// baseline in font design units.
    y_strikeout_position: BigEndian<i16>,
    /// This parameter is a classification of font-family design.
    s_family_class: BigEndian<i16>,
    /// Additional specifications are required for PANOSE to classify non-Latin
    /// character sets.
    panose_10: BigEndian<u8>,
    /// Unicode Character Range (bits 0-31).
    ul_unicode_range_1: BigEndian<u32>,
    /// Unicode Character Range (bits 32-63).
    ul_unicode_range_2: BigEndian<u32>,
    /// Unicode Character Range (bits 64-95).
    ul_unicode_range_3: BigEndian<u32>,
    /// Unicode Character Range (bits 96-127).
    ul_unicode_range_4: BigEndian<u32>,
    /// The four-character identifier for the vendor of the given type face.
    ach_vend_id: BigEndian<Tag>,
    /// Contains information concerning the nature of the font patterns.
    fs_selection: BigEndian<u16>,
    /// The minimum Unicode index (character code) in this font.
    us_first_char_index: BigEndian<u16>,
    /// The maximum Unicode index (character code) in this font.
    us_last_char_index: BigEndian<u16>,
    /// The typographic ascender for this font.
    s_typo_ascender: BigEndian<i16>,
    /// The typographic descender for this font.
    s_typo_decender: BigEndian<i16>,
    /// The typographic line gap for this font.
    s_typo_line_gap: BigEndian<i16>,
    /// The “Windows ascender” metric. This should be used to specify the
    /// height above the baseline for a clipping region.
    us_win_ascent: BigEndian<u16>,
    /// The “Windows descender” metric. This should be used to specify the
    /// vertical extend below the baseline for a clipping region.
    us_win_descent: BigEndian<u16>,

    /// Code page character range bits 0-31.
    #[available(Version16Dot16::VERSION_1_0)]
    ul_code_page_range_1: BigEndian<u32>,
    /// Code page character range bits 32-63.
    #[available(Version16Dot16::VERSION_1_0)]
    ul_code_page_range_2: BigEndian<u32>,

    /// This metric specifies the distance between the baseline and the
    /// approximate height of non-ascending lowercase letters measured in
    /// FUnits.
    #[available(Version16Dot16::VERSION_2_0)]
    sx_height: BigEndian<i16>,
    /// This metric specifies the distance between the baseline and the
    /// approximate height of uppercase letters measured in FUnits.
    #[available(Version16Dot16::VERSION_2_0)]
    s_cap_height: BigEndian<i16>,
    /// This is the Unicode code point, in UTF-16 encoding, of a character that
    /// can be used for a default glyph.
    #[available(Version16Dot16::VERSION_2_0)]
    us_default_char: BigEndian<u16>,
    /// his is the Unicode code point, in UTF-16 encoding, of a character that
    /// can be used as a default break character.
    #[available(Version16Dot16::VERSION_2_0)]
    us_break_char: BigEndian<u16>,
    /// This field is used for fonts with multiple optical styles.
    #[available(Version16Dot16::VERSION_2_0)]
    us_max_context: BigEndian<u16>,

    /// This field is used for fonts with multiple optical styles.
    #[available(Version16Dot16::VERSION_5_0)]
    us_lower_optical_point_size: BigEndian<u16>,
    /// This field is used for fonts with multiple optical styles.
    #[available(Version16Dot16::VERSION_5_0)]
    us_upper_optical_point_size: BigEndian<u16>,
}
