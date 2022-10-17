#![parse_module(read_fonts::tables::os2)]

/// [`os2`](https://docs.microsoft.com/en-us/typography/opentype/spec/os2)
table Os2 {
    /// The version: 0x00005000 for version 0.5, 0x00010000 for version 1.0.
    #[version]
    #[compile(self.compute_version())]
    version: BigEndian<Version16Dot16>,
    x_avg_char_width: BigEndian<i16>,
    us_weight_class: BigEndian<u16>,
    us_width_class: BigEndian<u16>,
    fs_type: BigEndian<u16>,
    y_subscript_x_size: BigEndian<i16>,
    y_subscript_y_size: BigEndian<i16>,
    y_subscript_x_offset: BigEndian<i16>,
    y_subscript_y_offset: BigEndian<i16>,
    y_superscript_x_size: BigEndian<i16>,
    y_superscript_y_size: BigEndian<i16>,
    y_superscript_x_offset: BigEndian<i16>,
    y_superscript_y_offset: BigEndian<i16>,
    y_strikeout_size: BigEndian<i16>,
    y_strikeout_position: BigEndian<i16>,
    s_family_class: BigEndian<i16>,
    panose_10: BigEndian<u8>,
    ul_unicode_range_1: BigEndian<u32>,
    ul_unicode_range_2: BigEndian<u32>,
    ul_unicode_range_3: BigEndian<u32>,
    ul_unicode_range_4: BigEndian<u32>,
    ach_vend_id: BigEndian<Tag>,
    fs_selection: BigEndian<u16>,
    us_first_char_index: BigEndian<u16>,
    us_last_char_index: BigEndian<u16>,
    s_typo_ascender: BigEndian<i16>,
    s_typo_decender: BigEndian<i16>,
    s_typo_line_gap: BigEndian<i16>,
    us_win_ascent: BigEndian<u16>,
    us_win_descent: BigEndian<u16>,

    #[available(Version16Dot16::VERSION_1_0)]
    ul_code_page_range_1: BigEndian<u32>,
    #[available(Version16Dot16::VERSION_1_0)]
    ul_code_page_range_2: BigEndian<u32>,

    #[available(Version16Dot16::VERSION_2_0)]
    sx_height: BigEndian<i16>,
    #[available(Version16Dot16::VERSION_2_0)]
    s_cap_height: BigEndian<i16>,
    #[available(Version16Dot16::VERSION_2_0)]
    us_default_char: BigEndian<u16>,
    #[available(Version16Dot16::VERSION_2_0)]
    us_break_char: BigEndian<u16>,
    #[available(Version16Dot16::VERSION_2_0)]
    us_max_context: BigEndian<u16>,

    #[available(Version16Dot16::VERSION_5_0)]
    us_lower_optical_point_size: BigEndian<u16>,
    #[available(Version16Dot16::VERSION_5_0)]
    us_upper_optical_point_size: BigEndian<u16>,
}
