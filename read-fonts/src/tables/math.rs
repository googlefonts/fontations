//! The [MATH](https://learn.microsoft.com/en-us/typography/opentype/spec/math) table

use super::layout::{CoverageTable, DeviceOrVariationIndex};

include!("../../generated/generated_math.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use font_test_data::bebuffer::BeBuffer;

    /// The MathValueRecords of MathConstants, in spec order, named as they are
    /// on the generated table.
    ///
    /// The test data gives the record at index `i` the value `100 + i`, so a
    /// field transcribed out of order reads back as one of its neighbours.
    const VALUE_RECORD_ORDER: &[&str] = &[
        "math_leading",
        "axis_height",
        "accent_base_height",
        "flattened_accent_base_height",
        "subscript_shift_down",
        "subscript_top_max",
        "subscript_baseline_drop_min",
        "superscript_shift_up",
        "superscript_shift_up_cramped",
        "superscript_bottom_min",
        "superscript_baseline_drop_max",
        "sub_superscript_gap_min",
        "superscript_bottom_max_with_subscript",
        "space_after_script",
        "upper_limit_gap_min",
        "upper_limit_baseline_rise_min",
        "lower_limit_gap_min",
        "lower_limit_baseline_drop_min",
        "stack_top_shift_up",
        "stack_top_display_style_shift_up",
        "stack_bottom_shift_down",
        "stack_bottom_display_style_shift_down",
        "stack_gap_min",
        "stack_display_style_gap_min",
        "stretch_stack_top_shift_up",
        "stretch_stack_bottom_shift_down",
        "stretch_stack_gap_above_min",
        "stretch_stack_gap_below_min",
        "fraction_numerator_shift_up",
        "fraction_numerator_display_style_shift_up",
        "fraction_denominator_shift_down",
        "fraction_denominator_display_style_shift_down",
        "fraction_numerator_gap_min",
        "fraction_num_display_style_gap_min",
        "fraction_rule_thickness",
        "fraction_denominator_gap_min",
        "fraction_denom_display_style_gap_min",
        "skewed_fraction_horizontal_gap",
        "skewed_fraction_vertical_gap",
        "overbar_vertical_gap",
        "overbar_rule_thickness",
        "overbar_extra_ascender",
        "underbar_vertical_gap",
        "underbar_rule_thickness",
        "underbar_extra_descender",
        "radical_vertical_gap",
        "radical_display_style_vertical_gap",
        "radical_rule_thickness",
        "radical_extra_ascender",
        "radical_kern_before_degree",
        "radical_kern_after_degree",
    ];

    /// A MATH table with all three subtables, built so that every value
    /// identifies the field it belongs to.
    fn math_table() -> BeBuffer {
        let mut buf = BeBuffer::new()
            // MATH header
            .push(MajorMinor::VERSION_1_0)
            .push_with_tag(0u16, "constants_offset")
            .push_with_tag(0u16, "glyph_info_offset")
            .push_with_tag(0u16, "variants_offset");

        // -- MathConstants --
        let constants = buf.len();
        buf = buf
            .push(1i16) // script_percent_scale_down
            .push(2i16) // script_script_percent_scale_down
            .push(3u16) // delimited_sub_formula_min_height
            .push(4u16); // display_operator_min_height
        for i in 0..VALUE_RECORD_ORDER.len() {
            buf = buf.push(100i16 + i as i16).push(0u16); // value, null device
        }
        buf = buf.push(5i16); // radical_degree_bottom_raise_percent

        // -- MathGlyphInfo --
        let glyph_info = buf.len();
        buf = buf
            .push_with_tag(0u16, "italics_offset")
            .push(0u16) // top accent attachment: null
            .push(0u16) // extended shape coverage: null
            .push_with_tag(0u16, "kern_info_offset");

        // MathItalicsCorrectionInfo, covering one glyph, with a device table.
        let italics = buf.len();
        buf = buf
            .push_with_tag(0u16, "italics_coverage_offset")
            .push(1u16) // count
            .push(37i16) // value
            .push_with_tag(0u16, "italics_device_offset");
        let italics_coverage = buf.len();
        buf = buf
            .push(1u16) // coverage format 1
            .push(1u16) // glyph count
            .push(9u16); // glyph 9
        let italics_device = buf.len();
        buf = buf
            .push(11u16) // start size
            .push(12u16) // end size
            .push(1u16) // delta format
            .push(0x1100u16); // delta values

        // MathKernInfo, one record carrying a single top-right kern.
        let kern_info = buf.len();
        buf = buf
            .push_with_tag(0u16, "kern_coverage_offset")
            .push(1u16) // count
            .push_with_tag(0u16, "top_right_kern_offset")
            .push(0u16) // top left: null
            .push(0u16) // bottom right: null
            .push(0u16); // bottom left: null
        let kern_coverage = buf.len();
        buf = buf.push(1u16).push(1u16).push(9u16);
        let math_kern = buf.len();
        buf = buf
            .push(2u16) // height_count
            .push(20i16) // correction height 0
            .push(0u16)
            .push(40i16) // correction height 1
            .push(0u16)
            .push(-1i16) // kern value 0
            .push(0u16)
            .push(-2i16) // kern value 1
            .push(0u16)
            .push(-3i16) // kern value 2, the height_count + 1 entry
            .push(0u16);

        // -- MathVariants --
        let variants = buf.len();
        buf = buf
            .push(6u16) // min_connector_overlap
            .push_with_tag(0u16, "vert_coverage_offset")
            .push(0u16) // horizontal coverage: null
            .push(1u16) // vert_glyph_count
            .push(0u16) // horiz_glyph_count
            .push_with_tag(0u16, "vert_construction_offset");
        let vert_coverage = buf.len();
        buf = buf.push(1u16).push(1u16).push(9u16);
        let construction = buf.len();
        buf = buf
            .push_with_tag(0u16, "assembly_offset")
            .push(2u16) // variant_count
            .push(21u16) // variant glyph
            .push(300u16) // advance
            .push(22u16)
            .push(400u16);
        let assembly = buf.len();
        buf = buf
            .push(17i16) // italics correction value
            .push(0u16) // italics correction device: null
            .push(1u16) // part_count
            .push(23u16) // glyph
            .push(10u16) // start connector
            .push(11u16) // end connector
            .push(50u16) // full advance
            .push(1u16); // part flags: extender

        buf.write_at("constants_offset", constants as u16);
        buf.write_at("glyph_info_offset", glyph_info as u16);
        buf.write_at("variants_offset", variants as u16);
        buf.write_at("italics_offset", (italics - glyph_info) as u16);
        buf.write_at("kern_info_offset", (kern_info - glyph_info) as u16);
        buf.write_at(
            "italics_coverage_offset",
            (italics_coverage - italics) as u16,
        );
        buf.write_at("italics_device_offset", (italics_device - italics) as u16);
        buf.write_at("kern_coverage_offset", (kern_coverage - kern_info) as u16);
        buf.write_at("top_right_kern_offset", (math_kern - kern_info) as u16);
        buf.write_at("vert_coverage_offset", (vert_coverage - variants) as u16);
        buf.write_at("vert_construction_offset", (construction - variants) as u16);
        buf.write_at("assembly_offset", (assembly - construction) as u16);
        buf
    }

    #[test]
    fn constants_are_in_spec_order() {
        let buf = math_table();
        let math = Math::read(buf.data().into()).unwrap();
        let constants = math.math_constants().unwrap();

        assert_eq!(constants.script_percent_scale_down(), 1);
        assert_eq!(constants.script_script_percent_scale_down(), 2);
        assert_eq!(constants.delimited_sub_formula_min_height(), UfWord::new(3));
        assert_eq!(constants.display_operator_min_height(), UfWord::new(4));
        assert_eq!(constants.radical_degree_bottom_raise_percent(), 5);

        // Each record carries its own index, so reading them in the order they
        // are named checks the whole run in one go.
        let values = [
            constants.math_leading(),
            constants.axis_height(),
            constants.accent_base_height(),
            constants.flattened_accent_base_height(),
            constants.subscript_shift_down(),
            constants.subscript_top_max(),
            constants.subscript_baseline_drop_min(),
            constants.superscript_shift_up(),
            constants.superscript_shift_up_cramped(),
            constants.superscript_bottom_min(),
            constants.superscript_baseline_drop_max(),
            constants.sub_superscript_gap_min(),
            constants.superscript_bottom_max_with_subscript(),
            constants.space_after_script(),
            constants.upper_limit_gap_min(),
            constants.upper_limit_baseline_rise_min(),
            constants.lower_limit_gap_min(),
            constants.lower_limit_baseline_drop_min(),
            constants.stack_top_shift_up(),
            constants.stack_top_display_style_shift_up(),
            constants.stack_bottom_shift_down(),
            constants.stack_bottom_display_style_shift_down(),
            constants.stack_gap_min(),
            constants.stack_display_style_gap_min(),
            constants.stretch_stack_top_shift_up(),
            constants.stretch_stack_bottom_shift_down(),
            constants.stretch_stack_gap_above_min(),
            constants.stretch_stack_gap_below_min(),
            constants.fraction_numerator_shift_up(),
            constants.fraction_numerator_display_style_shift_up(),
            constants.fraction_denominator_shift_down(),
            constants.fraction_denominator_display_style_shift_down(),
            constants.fraction_numerator_gap_min(),
            constants.fraction_num_display_style_gap_min(),
            constants.fraction_rule_thickness(),
            constants.fraction_denominator_gap_min(),
            constants.fraction_denom_display_style_gap_min(),
            constants.skewed_fraction_horizontal_gap(),
            constants.skewed_fraction_vertical_gap(),
            constants.overbar_vertical_gap(),
            constants.overbar_rule_thickness(),
            constants.overbar_extra_ascender(),
            constants.underbar_vertical_gap(),
            constants.underbar_rule_thickness(),
            constants.underbar_extra_descender(),
            constants.radical_vertical_gap(),
            constants.radical_display_style_vertical_gap(),
            constants.radical_rule_thickness(),
            constants.radical_extra_ascender(),
            constants.radical_kern_before_degree(),
            constants.radical_kern_after_degree(),
        ];
        assert_eq!(values.len(), VALUE_RECORD_ORDER.len());
        for (i, (value, name)) in values.iter().zip(VALUE_RECORD_ORDER).enumerate() {
            assert_eq!(
                value.value(),
                FWord::new(100 + i as i16),
                "{name} is not at spec index {i}"
            );
        }
    }

    #[test]
    fn italics_correction_device_resolves_against_its_parent() {
        let buf = math_table();
        let math = Math::read(buf.data().into()).unwrap();
        let info = math
            .math_glyph_info()
            .unwrap()
            .math_italics_correction_info()
            .unwrap()
            .unwrap();

        assert_eq!(info.coverage().unwrap().get(GlyphId::new(9)), Some(0));

        let correction = &info.italics_correction()[0];
        assert_eq!(correction.value(), FWord::new(37));
        // The offset is measured from MathItalicsCorrectionInfo rather than
        // from the record, so it only resolves against the parent's data.
        let device = correction.device(info.offset_data()).unwrap().unwrap();
        let DeviceOrVariationIndex::Device(device) = device else {
            panic!("expected a device table");
        };
        assert_eq!(device.start_size(), 11);
        assert_eq!(device.end_size(), 12);
    }

    #[test]
    fn math_kern_has_one_more_value_than_height() {
        let buf = math_table();
        let math = Math::read(buf.data().into()).unwrap();
        let kern_info = math
            .math_glyph_info()
            .unwrap()
            .math_kern_info()
            .unwrap()
            .unwrap();

        let record = &kern_info.math_kern_info_records()[0];
        let kern = record
            .top_right_math_kern(kern_info.offset_data())
            .unwrap()
            .unwrap();

        assert_eq!(kern.height_count(), 2);
        assert_eq!(
            kern.correction_height()
                .iter()
                .map(|v| v.value())
                .collect::<Vec<_>>(),
            vec![FWord::new(20), FWord::new(40)]
        );
        // Three kern values for two heights.
        assert_eq!(
            kern.kern_values()
                .iter()
                .map(|v| v.value())
                .collect::<Vec<_>>(),
            vec![FWord::new(-1), FWord::new(-2), FWord::new(-3)]
        );

        assert!(record.top_left_math_kern(kern_info.offset_data()).is_none());
    }

    #[test]
    fn variants_and_assembly() {
        let buf = math_table();
        let math = Math::read(buf.data().into()).unwrap();
        let variants = math.math_variants().unwrap();

        assert_eq!(variants.min_connector_overlap(), UfWord::new(6));
        assert_eq!(variants.vert_glyph_count(), 1);
        assert_eq!(variants.horiz_glyph_count(), 0);

        let construction = variants.vert_glyph_constructions().get(0).unwrap();
        assert_eq!(
            construction
                .math_glyph_variant_records()
                .iter()
                .map(|r| (r.variant_glyph(), r.advance_measurement()))
                .collect::<Vec<_>>(),
            vec![
                (GlyphId16::new(21), UfWord::new(300)),
                (GlyphId16::new(22), UfWord::new(400)),
            ]
        );

        let assembly = construction.glyph_assembly().unwrap().unwrap();
        assert_eq!(assembly.italics_correction().value(), FWord::new(17));
        let part = &assembly.part_records()[0];
        assert_eq!(part.glyph_id(), GlyphId16::new(23));
        assert_eq!(part.start_connector_length(), UfWord::new(10));
        assert_eq!(part.end_connector_length(), UfWord::new(11));
        assert_eq!(part.full_advance(), UfWord::new(50));
        assert!(part.part_flags().contains(PartFlags::EXTENDER_FLAG));
    }
}
