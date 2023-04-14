#![parse_module(read_fonts::tables::os2)]

/// OS/2 [selection flags](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fsselection)
flags u16 SelectionFlags {
    /// Bit 0: Font contains italic or oblique glyphs, otherwise they are
    /// upright. 
    ITALIC = 0x0001,
    /// Bit 1: Glyphs are underscored.
    UNDERSCORE = 0x0002,
    /// Bit 2: Glyphs have their foreground and background reversed.
    NEGATIVE = 0x0004,
    /// Bit 3: Outline (hollow) glyphs, otherwise they are solid.
    OUTLINED = 0x0008,
    /// Bit 4: Glyphs are overstruck.
    STRIKEOUT = 0x0010,
    /// Bit 5: Glyphs are emboldened.
    BOLD = 0x0020,
    /// Bit 6: Glyphs are in the standard weight/style for the font.
    REGULAR = 0x0040,
    /// Bit 7: If set, it is strongly recommended that applications use
    /// OS/2.sTypoAscender - OS/2.sTypoDescender + OS/2.sTypoLineGap as
    /// the default line spacing for this font.
    USE_TYPO_METRICS = 0x0080,
    /// Bit 8: The font has 'name' table strings consistent with a
    /// weight/width/slope family without requiring use of name IDs 21 and 22.
    WWS = 0x0100,
    /// Bit 9: Font contains oblique glyphs.
    OBLIQUE = 0x0200,
    // Bits 10-15 are reserved. Set to 0.
}

/// [`OS/2`](https://docs.microsoft.com/en-us/typography/opentype/spec/os2)
#[tag = "OS/2"]
#[skip_constructor]
table Os2 {
    #[version]
    #[compile(self.compute_version())]
    version: u16,
    /// [Average weighted escapement](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#xavgcharwidth).
    ///
    /// The Average Character Width parameter specifies the arithmetic average
    /// of the escapement (width) of all non-zero width glyphs in the font.
    x_avg_char_width: i16,
    /// [Weight class](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#usweightclass).
    ///
    /// Indicates the visual weight (degree of blackness or thickness of
    /// strokes) of the characters in the font. Values from 1 to 1000 are valid.
    #[default(400)]
    us_weight_class: u16,
    /// [Width class](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#uswidthclass).
    ///
    /// Indicates a relative change from the normal aspect ratio (width to height
    /// ratio) as specified by a font designer for the glyphs in a font.
    #[default(5)]
    us_width_class: u16,
    /// [Type flags](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fstype).
    ///
    /// Indicates font embedding licensing rights for the font.
    fs_type: u16,
    /// The recommended horizontal size in font design units for subscripts for
    /// this font.
    y_subscript_x_size: i16,
    /// The recommended vertical size in font design units for subscripts for
    /// this font.
    y_subscript_y_size: i16,
    /// The recommended horizontal offset in font design units for subscripts
    /// for this font.
    y_subscript_x_offset: i16,
    /// The recommended vertical offset in font design units for subscripts
    /// for this font.
    y_subscript_y_offset: i16,
    /// The recommended horizontal size in font design units for superscripts
    /// for this font.
    y_superscript_x_size: i16,
    /// The recommended vertical size in font design units for superscripts
    /// for this font.
    y_superscript_y_size: i16,
    /// The recommended horizontal offset in font design units for superscripts
    /// for this font.
    y_superscript_x_offset: i16,
    /// The recommended vertical offset in font design units for superscripts
    /// for this font.
    y_superscript_y_offset: i16,
    /// Thickness of the strikeout stroke in font design units.
    y_strikeout_size: i16,
    /// The position of the top of the strikeout stroke relative to the
    /// baseline in font design units.
    y_strikeout_position: i16,
    /// [Font-family class and subclass](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#sfamilyclass).
    /// This parameter is a classification of font-family design.
    s_family_class: i16,
    /// [PANOSE classification number](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#panose).
    ///
    /// Additional specifications are required for PANOSE to classify non-Latin
    /// character sets.
    #[count(10)]
    #[compile_type([u8; 10])]
    #[to_owned(convert_panose(obj.panose_10()))]
    panose_10: [u8],
    /// [Unicode Character Range](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#ulunicoderange1-bits-031ulunicoderange2-bits-3263ulunicoderange3-bits-6495ulunicoderange4-bits-96127).
    ///
    /// Unicode Character Range (bits 0-31).
    ul_unicode_range_1: u32,
    /// Unicode Character Range (bits 32-63).
    ul_unicode_range_2: u32,
    /// Unicode Character Range (bits 64-95).
    ul_unicode_range_3: u32,
    /// Unicode Character Range (bits 96-127).
    ul_unicode_range_4: u32,
    /// [Font Vendor Identification](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#achvendid).
    ///
    /// The four-character identifier for the vendor of the given type face.
    ach_vend_id: Tag,
    /// [Font selection flags](https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fsselection).
    ///
    /// Contains information concerning the nature of the font patterns.
    fs_selection: SelectionFlags,
    /// The minimum Unicode index (character code) in this font.
    us_first_char_index: u16,
    /// The maximum Unicode index (character code) in this font.
    us_last_char_index: u16,
    /// The typographic ascender for this font.
    s_typo_ascender: i16,
    /// The typographic descender for this font.
    s_typo_descender: i16,
    /// The typographic line gap for this font.
    s_typo_line_gap: i16,
    /// The “Windows ascender” metric.
    ///
    /// This should be used to specify the height above the baseline for a
    /// clipping region.
    us_win_ascent: u16,
    /// The “Windows descender” metric.
    ///
    /// This should be used to specify the vertical extent below the baseline
    /// for a clipping region.
    us_win_descent: u16,

    /// Code page character range bits 0-31.
    #[since_version(1)]
    ul_code_page_range_1: u32,
    /// Code page character range bits 32-63.
    #[since_version(1)]
    ul_code_page_range_2: u32,

    /// This metric specifies the distance between the baseline and the
    /// approximate height of non-ascending lowercase letters measured in
    /// FUnits.
    #[since_version(2)]
    sx_height: i16,
    /// This metric specifies the distance between the baseline and the
    /// approximate height of uppercase letters measured in FUnits.
    #[since_version(2)]
    s_cap_height: i16,
    /// This is the Unicode code point, in UTF-16 encoding, of a character that
    /// can be used for a default glyph.
    #[since_version(2)]
    us_default_char: u16,
    /// his is the Unicode code point, in UTF-16 encoding, of a character that
    /// can be used as a default break character.
    #[since_version(2)]
    us_break_char: u16,
    /// This field is used for fonts with multiple optical styles.
    #[since_version(2)]
    us_max_context: u16,

    /// This field is used for fonts with multiple optical styles.
    #[since_version(5)]
    us_lower_optical_point_size: u16,
    /// This field is used for fonts with multiple optical styles.
    #[since_version(5)]
    us_upper_optical_point_size: u16,
}
