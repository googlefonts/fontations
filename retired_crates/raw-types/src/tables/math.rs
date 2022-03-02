toy_table_macro::tables! {
MathValueRecord {
    /// The X or Y value in design units
    value: FWord,
    /// Offset to the device table — from the beginning of parent
    /// table. May be NULL. Suggested format for device table is 1.
    device_offset: Offset16,
}

Math {
    /// Major version of the MATH table, = 1.
    major_version: Uint16,
    /// Minor version of the MATH table, = 0.
    minor_version: Uint16,
    /// Offset to MathConstants table - from the beginning of MATH
    /// table.
    math_constants_offset: Offset16,
    /// Offset to MathGlyphInfo table - from the beginning of MATH
    /// table.
    math_glyph_info_offset: Offset16,
    /// Offset to MathVariants table - from the beginning of MATH table.
    math_variants_offset: Offset16,
}

MathConstants {
    /// Percentage of scaling down for level 1 superscripts and
    /// subscripts. Suggested value: 80%.
    script_percent_scale_down: Int16,
    /// Percentage of scaling down for level 2 (scriptScript)
    /// superscripts and subscripts. Suggested value: 60%.
    script_script_percent_scale_down: Int16,
    /// Minimum height required for a delimited expression (contained
    /// within parentheses, etc.) to be treated as a sub-formula.
    /// Suggested value: normal line height × 1.5.
    delimited_sub_formula_min_height: UFWord,
    /// Minimum height of n-ary operators (such as integral and
    /// summation) for formulas in display mode (that is, appearing as
    /// standalone page elements, not embedded inline within text).
    display_operator_min_height: UFWord,
    /// White space to be left between math formulas to ensure proper
    /// line spacing. For example, for applications that treat line gap
    /// as a part of line ascender, formulas with ink going above
    /// (os2.sTypoAscender + os2.sTypoLineGap - MathLeading) or with
    /// ink going below os2.sTypoDescender will result in increasing
    /// line height.
    math_leading: MathValueRecord,
    /// Axis height of the font. In math typesetting, the term axis
    /// refers to a horizontal reference line used for positioning
    /// elements in a formula. The math axis is similar to but distinct
    /// from the baseline for regular text layout. For example, in a
    /// simple equation, a minus symbol or fraction rule would be on
    /// the axis, but a string for a variable name would be set on a
    /// baseline that is offset from the axis. The axisHeight value
    /// determines the amount of that offset.
    axis_height: MathValueRecord,
    /// Maximum (ink) height of accent base that does not require
    /// raising the accents. Suggested: x‑height of the font
    /// (os2.sxHeight) plus any possible overshots.
    accent_base_height: MathValueRecord,
    /// Maximum (ink) height of accent base that does not require
    /// flattening the accents. Suggested: cap height of the font
    /// (os2.sCapHeight).
    flattened_accent_base_height: MathValueRecord,
    /// The standard shift down applied to subscript elements. Positive
    /// for moving in the downward direction. Suggested:
    /// os2.ySubscriptYOffset.
    subscript_shift_down: MathValueRecord,
    /// Maximum allowed height of the (ink) top of subscripts that does
    /// not require moving subscripts further down. Suggested: 4/5 x-
    /// height.
    subscript_top_max: MathValueRecord,
    /// Minimum allowed drop of the baseline of subscripts relative to
    /// the (ink) bottom of the base. Checked for bases that are
    /// treated as a box or extended shape. Positive for subscript
    /// baseline dropped below the base bottom.
    subscript_baseline_drop_min: MathValueRecord,
    /// Standard shift up applied to superscript elements. Suggested:
    /// os2.ySuperscriptYOffset.
    superscript_shift_up: MathValueRecord,
    /// Standard shift of superscripts relative to the base, in cramped
    /// style.
    superscript_shift_up_cramped: MathValueRecord,
    /// Minimum allowed height of the (ink) bottom of superscripts that
    /// does not require moving subscripts further up. Suggested: ¼
    /// x-height.
    superscript_bottom_min: MathValueRecord,
    /// Maximum allowed drop of the baseline of superscripts relative
    /// to the (ink) top of the base. Checked for bases that are
    /// treated as a box or extended shape. Positive for superscript
    /// baseline below the base top.
    superscript_baseline_drop_max: MathValueRecord,
    /// Minimum gap between the superscript and subscript ink.
    /// Suggested: 4 × default rule thickness.
    sub_superscript_gap_min: MathValueRecord,
    /// The maximum level to which the (ink) bottom of superscript can
    /// be pushed to increase the gap between superscript and
    /// subscript, before subscript starts being moved down. Suggested:
    /// 4/5 x-height.
    superscript_bottom_max_with_subscript: MathValueRecord,
    /// Extra white space to be added after each subscript and
    /// superscript. Suggested: 0.5 pt for a 12 pt font. (Note that, in
    /// some math layout implementations, a constant value, such as 0.5
    /// pt, may be used for all text sizes. Some implementations may
    /// use a constant ratio of text size, such as 1/24 of em.)
    space_after_script: MathValueRecord,
    /// Minimum gap between the (ink) bottom of the upper limit, and
    /// the (ink) top of the base operator.
    upper_limit_gap_min: MathValueRecord,
    /// Minimum distance between baseline of upper limit and (ink) top
    /// of the base operator.
    upper_limit_baseline_rise_min: MathValueRecord,
    /// Minimum gap between (ink) top of the lower limit, and (ink)
    /// bottom of the base operator.
    lower_limit_gap_min: MathValueRecord,
    /// Minimum distance between baseline of the lower limit and (ink)
    /// bottom of the base operator.
    lower_limit_baseline_drop_min: MathValueRecord,
    /// Standard shift up applied to the top element of a stack.
    stack_top_shift_up: MathValueRecord,
    /// Standard shift up applied to the top element of a stack in
    /// display style.
    stack_top_display_style_shift_up: MathValueRecord,
    /// Standard shift down applied to the bottom element of a stack.
    /// Positive for moving in the downward direction.
    stack_bottom_shift_down: MathValueRecord,
    /// Standard shift down applied to the bottom element of a stack in
    /// display style. Positive for moving in the downward direction.
    stack_bottom_display_style_shift_down: MathValueRecord,
    /// Minimum gap between (ink) bottom of the top element of a stack,
    /// and the (ink) top of the bottom element. Suggested: 3 ×
    /// default rule thickness.
    stack_gap_min: MathValueRecord,
    /// Minimum gap between (ink) bottom of the top element of a stack,
    /// and the (ink) top of the bottom element in display style.
    /// Suggested: 7 × default rule thickness.
    stack_display_style_gap_min: MathValueRecord,
    /// Standard shift up applied to the top element of the stretch
    /// stack.
    stretch_stack_top_shift_up: MathValueRecord,
    /// Standard shift down applied to the bottom element of the
    /// stretch stack. Positive for moving in the downward direction.
    stretch_stack_bottom_shift_down: MathValueRecord,
    /// Minimum gap between the ink of the stretched element, and the
    /// (ink) bottom of the element above. Suggested: same value as
    /// upperLimitGapMin.
    stretch_stack_gap_above_min: MathValueRecord,
    /// Minimum gap between the ink of the stretched element, and the
    /// (ink) top of the element below. Suggested: same value as
    /// lowerLimitGapMin.
    stretch_stack_gap_below_min: MathValueRecord,
    /// Standard shift up applied to the numerator.
    fraction_numerator_shift_up: MathValueRecord,
    /// Standard shift up applied to the numerator in display style.
    /// Suggested: same value as stackTopDisplayStyleShiftUp.
    fraction_numerator_display_style_shift_up: MathValueRecord,
    /// Standard shift down applied to the denominator. Positive for
    /// moving in the downward direction.
    fraction_denominator_shift_down: MathValueRecord,
    /// Standard shift down applied to the denominator in display
    /// style. Positive for moving in the downward direction.
    /// Suggested: same value as stackBottomDisplayStyleShiftDown.
    fraction_denominator_display_style_shift_down: MathValueRecord,
    /// Minimum tolerated gap between the (ink) bottom of the numerator
    /// and the ink of the fraction bar. Suggested: default rule
    /// thickness.
    fraction_numerator_gap_min: MathValueRecord,
    /// Minimum tolerated gap between the (ink) bottom of the numerator
    /// and the ink of the fraction bar in display style. Suggested: 3
    /// × default rule thickness.
    fraction_num_display_style_gap_min: MathValueRecord,
    /// Thickness of the fraction bar. Suggested: default rule
    /// thickness.
    fraction_rule_thickness: MathValueRecord,
    /// Minimum tolerated gap between the (ink) top of the denominator
    /// and the ink of the fraction bar. Suggested: default rule
    /// thickness.
    fraction_denominator_gap_min: MathValueRecord,
    /// Minimum tolerated gap between the (ink) top of the denominator
    /// and the ink of the fraction bar in display style. Suggested: 3
    /// × default rule thickness.
    fraction_denom_display_style_gap_min: MathValueRecord,
    /// Horizontal distance between the top and bottom elements of a
    /// skewed fraction.
    skewed_fraction_horizontal_gap: MathValueRecord,
    /// Vertical distance between the ink of the top and bottom
    /// elements of a skewed fraction.
    skewed_fraction_vertical_gap: MathValueRecord,
    /// Distance between the overbar and the (ink) top of he base.
    /// Suggested: 3 × default rule thickness.
    overbar_vertical_gap: MathValueRecord,
    /// Thickness of overbar. Suggested: default rule thickness.
    overbar_rule_thickness: MathValueRecord,
    /// Extra white space reserved above the overbar. Suggested:
    /// default rule thickness.
    overbar_extra_ascender: MathValueRecord,
    /// Distance between underbar and (ink) bottom of the base.
    /// Suggested: 3 × default rule thickness.
    underbar_vertical_gap: MathValueRecord,
    /// Thickness of underbar. Suggested: default rule thickness.
    underbar_rule_thickness: MathValueRecord,
    /// Extra white space reserved below the underbar. Always positive.
    /// Suggested: default rule thickness.
    underbar_extra_descender: MathValueRecord,
    /// Space between the (ink) top of the expression and the bar over
    /// it. Suggested: 1¼ default rule thickness.
    radical_vertical_gap: MathValueRecord,
    /// Space between the (ink) top of the expression and the bar over
    /// it. Suggested: default rule thickness + ¼ x-height.
    radical_display_style_vertical_gap: MathValueRecord,
    /// Thickness of the radical rule. This is the thickness of the
    /// rule in designed or constructed radical signs. Suggested:
    /// default rule thickness.
    radical_rule_thickness: MathValueRecord,
    /// Extra white space reserved above the radical. Suggested: same
    /// value as radicalRuleThickness.
    radical_extra_ascender: MathValueRecord,
    /// Extra horizontal kern before the degree of a radical, if such
    /// is present. Suggested: 5/18 of em.
    radical_kern_before_degree: MathValueRecord,
    /// Negative kern after the degree of a radical, if such is
    /// present. Suggested: −10/18 of em.
    radical_kern_after_degree: MathValueRecord,
    /// Height of the bottom of the radical degree, if such is present,
    /// in proportion to the ascender of the radical sign. Suggested:
    /// 60%.
    radical_degree_bottom_raise_percent: Int16,
}

MathGlyphInfo {
    /// Offset to MathItalicsCorrectionInfo table, from the beginning
    /// of the MathGlyphInfo table.
    math_italics_correction_info_offset: Offset16,
    /// Offset to MathTopAccentAttachment table, from the beginning of
    /// the MathGlyphInfo table.
    math_top_accent_attachment_offset: Offset16,
    /// Offset to ExtendedShapes coverage table, from the beginning of
    /// the MathGlyphInfo table. When the glyph to the left or right of
    /// a box is an extended shape variant, the (ink) box should be
    /// used for vertical positioning purposes, not the default
    /// position defined by values in MathConstants table. May be NULL.
    extended_shape_coverage_offset: Offset16,
    /// Offset to MathKernInfo table, from the beginning of the
    /// MathGlyphInfo table.
    math_kern_info_offset: Offset16,
}

MathItalicsCorrectionInfo<'a> {
    /// Offset to Coverage table - from the beginning of
    /// MathItalicsCorrectionInfo table.
    italics_correction_coverage_offset: Offset16,
    /// Number of italics correction values. Should coincide with the
    /// number of covered glyphs.
    italics_correction_count: Uint16,
    /// Array of MathValueRecords defining italics correction values
    /// for each covered glyph.
    #[count(italics_correction_count)]
    italics_correction: [MathValueRecord],
}

MathTopAccentAttachment<'a> {
    /// Offset to Coverage table, from the beginning of the
    /// MathTopAccentAttachment table.
    top_accent_coverage_offset: Offset16,
    /// Number of top accent attachment point values. Must be the same
    /// as the number of glyph IDs referenced in the Coverage table.
    top_accent_attachment_count: Uint16,
    /// Array of MathValueRecords defining top accent attachment points
    /// for each covered glyph
    #[count(top_accent_attachment_count)]
    top_accent_attachment: [MathValueRecord],
}

MathKernInfo<'a> {
    /// Offset to Coverage table, from the beginning of the
    /// MathKernInfo table.
    math_kern_coverage_offset: Offset16,
    /// Number of MathKernInfoRecords. Must be the same as the number
    /// of glyph IDs referenced in the Coverage table.
    math_kern_count: Uint16,
    /// Array of MathKernInfoRecords, one for each covered glyph.
    #[count(math_kern_count)]
    math_kern_info_records: [MathKernInfoRecord],
}

MathKernInfoRecord {
    /// Offset to MathKern table for top right corner, from the
    /// beginning of the MathKernInfo table. May be NULL.
    top_right_math_kern_offset: Offset16,
    /// Offset to MathKern table for the top left corner, from the
    /// beginning of the MathKernInfo table. May be NULL.
    top_left_math_kern_offset: Offset16,
    /// Offset to MathKern table for bottom right corner, from the
    /// beginning of the MathKernInfo table. May be NULL.
    bottom_right_math_kern_offset: Offset16,
    /// Offset to MathKern table for bottom left corner, from the
    /// beginning of the MathKernInfo table. May be NULL.
    bottom_left_math_kern_offset: Offset16,
}

MathKern<'a> {
    /// Number of heights at which the kern value changes.
    height_count: Uint16,
    /// Array of correction heights, in design units, sorted from
    /// lowest to highest.
    #[count(height_count)]
    correction_height: [MathValueRecord],
    /// + 1]    Array of kerning values for different height ranges.
    /// Negative values are used to move glyphs closer to each other.
    #[count(height_count)]
    kern_values: [MathValueRecord],
}

MathVariants<'a> {
    /// Minimum overlap of connecting glyphs during glyph construction,
    /// in design units.
    min_connector_overlap: UFWord,
    /// Offset to Coverage table, from the beginning of the
    /// MathVariants table.
    vert_glyph_coverage_offset: Offset16,
    /// Offset to Coverage table, from the beginning of the
    /// MathVariants table.
    horiz_glyph_coverage_offset: Offset16,
    /// Number of glyphs for which information is provided for
    /// vertically growing variants. Must be the same as the number of
    /// glyph IDs referenced in the vertical Coverage table.
    vert_glyph_count: Uint16,
    /// Number of glyphs for which information is provided for
    /// horizontally growing variants. Must be the same as the number
    /// of glyph IDs referenced in the horizontal Coverage table.
    horiz_glyph_count: Uint16,
    /// Array of offsets to MathGlyphConstruction tables, from the
    /// beginning of the MathVariants table, for shapes growing in the
    /// vertical direction.
    #[count(vert_glyph_count)]
    vert_glyph_construction_offsets: [Offset16],
    /// Array of offsets to MathGlyphConstruction tables, from the
    /// beginning of the MathVariants table, for shapes growing in the
    /// horizontal direction.
    #[count(horiz_glyph_count)]
    horiz_glyph_construction_offsets: [Offset16],
}

MathGlyphConstruction<'a> {
    /// Offset to the GlyphAssembly table for this shape, from the
    /// beginning of the MathGlyphConstruction table. May be NULL.
    glyph_assembly_offset: Offset16,
    /// Count of glyph growing variants for this glyph.
    variant_count: Uint16,
    /// MathGlyphVariantRecords for alternative variants of the glyphs.
    #[count(variant_count)]
    math_glyph_variant_record: [MathGlyphVariantRecord],
}

MathGlyphVariantRecord {
    /// Glyph ID for the variant.
    variant_glyph: Uint16,
    /// Advance width/height, in design units, of the variant, in the
    /// direction of requested glyph extension.
    advance_measurement: UFWord,
}

GlyphAssembly<'a> {
    /// Italics correction of this GlyphAssembly. Should not depend on
    /// the assembly size.
    italics_correction: MathValueRecord,
    /// Number of parts in this assembly.
    part_count: Uint16,
    /// Array of part records, from left to right (for assemblies that
    /// extend horizontally) or bottom to top (for assemblies that
    /// extend vertically).
    #[count(part_count)]
    part_records: [GlyphPartRecord],
}

GlyphPartRecord {
    /// Glyph ID for the part.
    glyph_i_d: Uint16,
    /// Advance width/ height, in design units, of the straight bar
    /// connector material at the start of the glyph in the direction
    /// of the extension (the left end for horizontal extension, the
    /// bottom end for vertical extension).
    start_connector_length: UFWord,
    /// Advance width/ height, in design units, of the straight bar
    /// connector material at the end of the glyph in the direction of
    /// the extension (the right end for horizontal extension, the top
    /// end for vertical extension).
    end_connector_length: UFWord,
    /// Full advance width/height for this part in the direction of the
    /// extension, in design units.
    full_advance: UFWord,
    /// Part qualifiers. PartFlags enumeration currently uses only one
    /// bit: 0x0001 EXTENDER_FLAG: If set, the part can be skipped or
    /// repeated. 0xFFFE Reserved.
    part_flags: Uint16,
}

}
