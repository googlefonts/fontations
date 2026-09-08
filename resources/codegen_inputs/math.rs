#![parse_module(read_fonts::tables::math)]

/// The [MATH](https://learn.microsoft.com/en-us/typography/opentype/spec/math) table
#[tag = "MATH"]
table Math {
    /// (major, minor) Version for the MATH table (1,0)
    #[version]
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Offset to MathConstants table, from beginning of MATH table.
    math_constants_offset: Offset16<MathConstants>,
    /// Offset to MathGlyphInfo table, from beginning of MATH table.
    math_glyph_info_offset: Offset16<MathGlyphInfo>,
    /// Offset to MathVariants table, from beginning of MATH table.
    math_variants_offset: Offset16<MathVariants>,
}

/// [MathValueRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathvaluerecord)
///
/// The device offset is measured from the beginning of the table that contains
/// the record, not from the record itself.
record MathValueRecord {
    /// The X or Y value in design units.
    value: FWord,
    /// Offset to the Device table (non-variable font) / VariationIndex table
    /// (variable font) for this value, from the beginning of the parent table.
    /// May be NULL.
    #[nullable]
    device_offset: Offset16<DeviceOrVariationIndex>,
}

/// [MathConstants](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathconstants-table)
table MathConstants {
    /// Percentage of scaling down for level 1 superscripts and subscripts.
    script_percent_scale_down: i16,
    /// Percentage of scaling down for level 2 (scriptScript) superscripts and
    /// subscripts.
    script_script_percent_scale_down: i16,
    /// Minimum height required for a delimited expression (contained within
    /// parentheses, etc.) to be treated as a sub-formula.
    delimited_sub_formula_min_height: UfWord,
    /// Minimum height of n-ary operators (such as integral and summation) for
    /// formulas in display mode (that is, appearing as standalone page
    /// elements, not embedded inline within text).
    display_operator_min_height: UfWord,
    /// White space to be left between math formulas to ensure proper line
    /// spacing.
    math_leading: MathValueRecord,
    /// Axis height of the font.
    axis_height: MathValueRecord,
    /// Maximum (ink) height of accent base that does not require raising the
    /// accents.
    accent_base_height: MathValueRecord,
    /// Maximum (ink) height of accent base that does not require flattening the
    /// accents.
    flattened_accent_base_height: MathValueRecord,
    /// The standard shift down applied to subscript elements.
    subscript_shift_down: MathValueRecord,
    /// Maximum allowed height of the (ink) top of subscripts that does not
    /// require moving subscripts further down.
    subscript_top_max: MathValueRecord,
    /// Minimum allowed drop of the baseline of subscripts relative to the (ink)
    /// bottom of the base.
    subscript_baseline_drop_min: MathValueRecord,
    /// Standard shift up applied to superscript elements.
    superscript_shift_up: MathValueRecord,
    /// Standard shift of superscripts relative to the base, in cramped style.
    superscript_shift_up_cramped: MathValueRecord,
    /// Minimum allowed height of the (ink) bottom of superscripts that does not
    /// require moving subscripts further up.
    superscript_bottom_min: MathValueRecord,
    /// Maximum allowed drop of the baseline of superscripts relative to the
    /// (ink) top of the base.
    superscript_baseline_drop_max: MathValueRecord,
    /// Minimum gap between the superscript and subscript ink.
    sub_superscript_gap_min: MathValueRecord,
    /// The maximum level to which the (ink) bottom of superscript can be
    /// pushed to increase the gap between superscript and subscript, before
    /// subscript starts being moved down.
    superscript_bottom_max_with_subscript: MathValueRecord,
    /// Extra white space to be added after each subscript and superscript.
    space_after_script: MathValueRecord,
    /// Minimum gap between the (ink) bottom of the upper limit, and the (ink)
    /// top of the base operator.
    upper_limit_gap_min: MathValueRecord,
    /// Minimum distance between baseline of upper limit and (ink) top of the
    /// base operator.
    upper_limit_baseline_rise_min: MathValueRecord,
    /// Minimum gap between (ink) top of the lower limit, and (ink) bottom of
    /// the base operator.
    lower_limit_gap_min: MathValueRecord,
    /// Minimum distance between baseline of the lower limit and (ink) bottom of
    /// the base operator.
    lower_limit_baseline_drop_min: MathValueRecord,
    /// Standard shift up applied to the top element of a stack.
    stack_top_shift_up: MathValueRecord,
    /// Standard shift up applied to the top element of a stack in display
    /// style.
    stack_top_display_style_shift_up: MathValueRecord,
    /// Standard shift down applied to the bottom element of a stack.
    stack_bottom_shift_down: MathValueRecord,
    /// Standard shift down applied to the bottom element of a stack in display
    /// style.
    stack_bottom_display_style_shift_down: MathValueRecord,
    /// Minimum gap between (ink) bottom of the top element of a stack, and the
    /// (ink) top of the bottom element.
    stack_gap_min: MathValueRecord,
    /// Minimum gap between (ink) bottom of the top element of a stack, and the
    /// (ink) top of the bottom element in display style.
    stack_display_style_gap_min: MathValueRecord,
    /// Standard shift up applied to the top element of the stretch stack.
    stretch_stack_top_shift_up: MathValueRecord,
    /// Standard shift down applied to the bottom element of the stretch stack.
    stretch_stack_bottom_shift_down: MathValueRecord,
    /// Minimum gap between the ink of the stretched element, and the (ink)
    /// bottom of the element above.
    stretch_stack_gap_above_min: MathValueRecord,
    /// Minimum gap between the ink of the stretched element, and the (ink) top
    /// of the element below.
    stretch_stack_gap_below_min: MathValueRecord,
    /// Standard shift up applied to the numerator.
    fraction_numerator_shift_up: MathValueRecord,
    /// Standard shift up applied to the numerator in display style.
    fraction_numerator_display_style_shift_up: MathValueRecord,
    /// Standard shift down applied to the denominator.
    fraction_denominator_shift_down: MathValueRecord,
    /// Standard shift down applied to the denominator in display style.
    fraction_denominator_display_style_shift_down: MathValueRecord,
    /// Minimum tolerated gap between the (ink) bottom of the numerator and the
    /// ink of the fraction bar.
    fraction_numerator_gap_min: MathValueRecord,
    /// Minimum tolerated gap between the (ink) bottom of the numerator and the
    /// ink of the fraction bar in display style.
    fraction_num_display_style_gap_min: MathValueRecord,
    /// Thickness of the fraction bar.
    fraction_rule_thickness: MathValueRecord,
    /// Minimum tolerated gap between the ink of the fraction bar and the (ink)
    /// top of the denominator.
    fraction_denominator_gap_min: MathValueRecord,
    /// Minimum tolerated gap between the ink of the fraction bar and the (ink)
    /// top of the denominator in display style.
    fraction_denom_display_style_gap_min: MathValueRecord,
    /// Horizontal distance between the top and bottom elements of a skewed
    /// fraction.
    skewed_fraction_horizontal_gap: MathValueRecord,
    /// Vertical distance between the ink of the top and bottom elements of a
    /// skewed fraction.
    skewed_fraction_vertical_gap: MathValueRecord,
    /// Distance between the overbar and the (ink) top of he base.
    overbar_vertical_gap: MathValueRecord,
    /// Thickness of overbar.
    overbar_rule_thickness: MathValueRecord,
    /// Extra white space reserved above the overbar.
    overbar_extra_ascender: MathValueRecord,
    /// Distance between underbar and (ink) bottom of the base.
    underbar_vertical_gap: MathValueRecord,
    /// Thickness of underbar.
    underbar_rule_thickness: MathValueRecord,
    /// Extra white space reserved below the underbar.
    underbar_extra_descender: MathValueRecord,
    /// Space between the (ink) top of the expression and the bar over it.
    radical_vertical_gap: MathValueRecord,
    /// Space between the (ink) top of the expression and the bar over it in
    /// display style.
    radical_display_style_vertical_gap: MathValueRecord,
    /// Thickness of the radical rule.
    radical_rule_thickness: MathValueRecord,
    /// Extra white space reserved above the radical.
    radical_extra_ascender: MathValueRecord,
    /// Extra horizontal kern before the degree of a radical, if such is
    /// present.
    radical_kern_before_degree: MathValueRecord,
    /// Negative kern after the degree of a radical, if such is present.
    radical_kern_after_degree: MathValueRecord,
    /// Height of the bottom of the radical degree, if such is present, in
    /// proportion to the ascender of the radical sign.
    radical_degree_bottom_raise_percent: i16,
}

/// [MathGlyphInfo](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathglyphinfo-table)
table MathGlyphInfo {
    /// Offset to MathItalicsCorrectionInfo table, from the beginning of the
    /// MathGlyphInfo table. May be NULL.
    #[nullable]
    math_italics_correction_info_offset: Offset16<MathItalicsCorrectionInfo>,
    /// Offset to MathTopAccentAttachment table, from the beginning of the
    /// MathGlyphInfo table. May be NULL.
    #[nullable]
    math_top_accent_attachment_offset: Offset16<MathTopAccentAttachment>,
    /// Offset to the Coverage table for glyphs with extended shapes, from the
    /// beginning of the MathGlyphInfo table. May be NULL.
    #[nullable]
    extended_shape_coverage_offset: Offset16<CoverageTable>,
    /// Offset to the MathKernInfo table, from the beginning of the
    /// MathGlyphInfo table. May be NULL.
    #[nullable]
    math_kern_info_offset: Offset16<MathKernInfo>,
}

/// [MathItalicsCorrectionInfo](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathitalicscorrectioninfo-table)
table MathItalicsCorrectionInfo {
    /// Offset to the Coverage table, from the beginning of the
    /// MathItalicsCorrectionInfo table.
    coverage_offset: Offset16<CoverageTable>,
    /// Number of italics correction values. Should coincide with the number of
    /// covered glyphs.
    #[compile(array_len($italics_correction))]
    italics_correction_count: u16,
    /// Array of MathValueRecords defining italics correction values for each
    /// covered glyph.
    #[count($italics_correction_count)]
    italics_correction: [MathValueRecord],
}

/// [MathTopAccentAttachment](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathtopaccentattachment-table)
table MathTopAccentAttachment {
    /// Offset to the Coverage table, from the beginning of the
    /// MathTopAccentAttachment table.
    top_accent_coverage_offset: Offset16<CoverageTable>,
    /// Number of top accent attachment point values. Should coincide with the
    /// number of covered glyphs.
    #[compile(array_len($top_accent_attachment))]
    top_accent_attachment_count: u16,
    /// Array of MathValueRecords defining top accent attachment points for each
    /// covered glyph.
    #[count($top_accent_attachment_count)]
    top_accent_attachment: [MathValueRecord],
}

/// [MathKernInfo](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathkerninfo-table)
table MathKernInfo {
    /// Offset to the Coverage table, from the beginning of the MathKernInfo
    /// table.
    math_kern_coverage_offset: Offset16<CoverageTable>,
    /// Number of MathKernInfoRecords. Should coincide with the number of
    /// covered glyphs.
    #[compile(array_len($math_kern_info_records))]
    math_kern_count: u16,
    /// Array of MathKernInfoRecords, one for each covered glyph.
    #[count($math_kern_count)]
    math_kern_info_records: [MathKernInfoRecord],
}

/// [MathKernInfoRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathkerninforecord)
///
/// The offsets are measured from the beginning of the MathKernInfo table.
record MathKernInfoRecord {
    /// Offset to the MathKern table for the top right corner, from the
    /// beginning of the MathKernInfo table. May be NULL.
    #[nullable]
    top_right_math_kern_offset: Offset16<MathKern>,
    /// Offset to the MathKern table for the top left corner, from the beginning
    /// of the MathKernInfo table. May be NULL.
    #[nullable]
    top_left_math_kern_offset: Offset16<MathKern>,
    /// Offset to the MathKern table for the bottom right corner, from the
    /// beginning of the MathKernInfo table. May be NULL.
    #[nullable]
    bottom_right_math_kern_offset: Offset16<MathKern>,
    /// Offset to the MathKern table for the bottom left corner, from the
    /// beginning of the MathKernInfo table. May be NULL.
    #[nullable]
    bottom_left_math_kern_offset: Offset16<MathKern>,
}

/// [MathKern](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathkern-table)
table MathKern {
    /// Number of heights at which the kern value changes.
    #[compile(array_len($correction_height))]
    height_count: u16,
    /// Array of correction heights, in design units, sorted from lowest to
    /// highest.
    #[count($height_count)]
    correction_height: [MathValueRecord],
    /// Array of kerning values for different height ranges, in design units.
    ///
    /// There is one more of these than there are correction heights: the first
    /// applies below the first correction height, and the last above the last.
    #[count(add($height_count, 1))]
    kern_values: [MathValueRecord],
}

/// [MathVariants](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathvariants-table)
table MathVariants {
    /// Minimum overlap of connecting glyphs during glyph construction, in
    /// design units.
    min_connector_overlap: UfWord,
    /// Offset to the Coverage table for shapes growing in the vertical
    /// direction, from the beginning of the MathVariants table. May be NULL.
    #[nullable]
    vert_glyph_coverage_offset: Offset16<CoverageTable>,
    /// Offset to the Coverage table for shapes growing in the horizontal
    /// direction, from the beginning of the MathVariants table. May be NULL.
    #[nullable]
    horiz_glyph_coverage_offset: Offset16<CoverageTable>,
    /// Number of glyphs for which information is provided for vertically
    /// growing variants. Must be the same as the number of glyph IDs referenced
    /// in the vertical Coverage table.
    #[compile(array_len($vert_glyph_construction_offsets))]
    vert_glyph_count: u16,
    /// Number of glyphs for which information is provided for horizontally
    /// growing variants. Must be the same as the number of glyph IDs referenced
    /// in the horizontal Coverage table.
    #[compile(array_len($horiz_glyph_construction_offsets))]
    horiz_glyph_count: u16,
    /// Array of offsets to MathGlyphConstruction tables, from the beginning of
    /// the MathVariants table, for shapes growing in the vertical direction.
    #[count($vert_glyph_count)]
    vert_glyph_construction_offsets: [Offset16<MathGlyphConstruction>],
    /// Array of offsets to MathGlyphConstruction tables, from the beginning of
    /// the MathVariants table, for shapes growing in the horizontal direction.
    #[count($horiz_glyph_count)]
    horiz_glyph_construction_offsets: [Offset16<MathGlyphConstruction>],
}

/// [MathGlyphConstruction](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathglyphconstruction-table)
table MathGlyphConstruction {
    /// Offset to the GlyphAssembly table for this shape, from the beginning of
    /// the MathGlyphConstruction table. May be NULL.
    #[nullable]
    glyph_assembly_offset: Offset16<GlyphAssembly>,
    /// Count of glyph growing variants for this glyph.
    #[compile(array_len($math_glyph_variant_records))]
    variant_count: u16,
    /// MathGlyphVariantRecords for alternative variants of the base glyph.
    #[count($variant_count)]
    math_glyph_variant_records: [MathGlyphVariantRecord],
}

/// [MathGlyphVariantRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/math#mathglyphvariantrecord)
record MathGlyphVariantRecord {
    /// Glyph ID for the variant.
    variant_glyph: GlyphId16,
    /// Advance width/height, in design units, of the variant, in the direction
    /// of requested glyph extension.
    advance_measurement: UfWord,
}

/// [GlyphAssembly](https://learn.microsoft.com/en-us/typography/opentype/spec/math#glyphassembly-table)
table GlyphAssembly {
    /// Italics correction of this GlyphAssembly. Should not depend on the
    /// assembly size.
    italics_correction: MathValueRecord,
    /// Number of parts in this assembly.
    #[compile(array_len($part_records))]
    part_count: u16,
    /// Array of part records, from left to right (for assemblies that extend
    /// horizontally) or bottom to top (for assemblies that extend vertically).
    #[count($part_count)]
    part_records: [GlyphPartRecord],
}

/// [GlyphPartRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/math#glyphpartrecord)
record GlyphPartRecord {
    /// Glyph ID for the part.
    glyph_id: GlyphId16,
    /// Advance width/ height, in design units, of the straight bar connector
    /// material at the start of the glyph in the direction of the extension.
    start_connector_length: UfWord,
    /// Advance width/ height, in design units, of the straight bar connector
    /// material at the end of the glyph in the direction of the extension.
    end_connector_length: UfWord,
    /// Full advance width/height for this part in the direction of the
    /// extension, in design units.
    full_advance: UfWord,
    /// Part qualifiers.
    part_flags: PartFlags,
}

/// Qualifiers for a [`GlyphPartRecord`].
flags u16 PartFlags {
    /// If set, the part can be skipped or repeated.
    EXTENDER_FLAG = 0x0001,
}
