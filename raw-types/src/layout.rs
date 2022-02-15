//! OpenType Layout Common Table Formats
//!
//! See [the docs](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2).

use raw_types::{Offset16, Offset32, Uint16};

toy_table_macro::tables! {
    ScriptList<'a> {
        /// Number of ScriptRecords
        script_count: Uint16,
        /// Array of ScriptRecords, listed alphabetically by script tag
        #[count(script_count)]
        script_records: [ScriptRecord],
    }

    ScriptRecord {
        /// 4-byte script tag identifier
        script_tag: Tag,
        /// Offset to Script table, from beginning of ScriptList
        script_offset: Offset16,
    }

    Script<'a> {
        /// Offset to default LangSys table, from beginning of Script table
        /// — may be NULL
        default_lang_sys_offset: Offset16,
        /// Number of LangSysRecords for this script — excluding the
        /// default LangSys
        lang_sys_count: Uint16,
        /// Array of LangSysRecords, listed alphabetically by LangSys tag
        #[count(lang_sys_count)]
        lang_sys_records: [LangSysRecord],
    }

    LangSysRecord {
        /// 4-byte LangSysTag identifier
        lang_sys_tag: Tag,
        /// Offset to LangSys table, from beginning of Script table
        lang_sys_offset: Offset16,
    }

    LangSys<'a> {
        /// = NULL (reserved for an offset to a reordering table)
        lookup_order_offset: Offset16,
        /// Index of a feature required for this language system; if no
        /// required features = 0xFFFF
        required_feature_index: Uint16,
        /// Number of feature index values for this language system —
        /// excludes the required feature
        feature_index_count: Uint16,
        /// Array of indices into the FeatureList, in arbitrary order
        #[count(feature_index_count)]
        feature_indices: [Uint16],
    }
}

toy_table_macro::tables! {
    FeatureList<'a> {
        /// Number of FeatureRecords in this table
        feature_count: Uint16,
        /// Array of FeatureRecords — zero-based (first feature has
        /// FeatureIndex = 0), listed alphabetically by feature tag
        #[count(feature_count)]
        feature_records: [FeatureRecord],
    }

    FeatureRecord {
        /// 4-byte feature identification tag
        feature_tag: Tag,
        /// Offset to Feature table, from beginning of FeatureList
        feature_offset: Offset16,
    }

    Feature<'a> {
        /// Number of LookupList indices for this feature
        lookup_index_count: Uint16,
        /// Array of indices into the LookupList — zero-based (first
        /// lookup is LookupListIndex = 0)
        #[count(lookup_index_count)]
        lookup_list_indices: [Uint16],
    }

    LookupList<'a> {
        /// Number of lookups in this table
        lookup_count: Uint16,
        /// Array of offsets to Lookup tables, from beginning of LookupList
        /// — zero based (first lookup is Lookup index = 0)
        #[count(lookup_count)]
        lookup_offsets: [Offset16],
    }

    Lookup<'a> {
        /// Different enumerations for GSUB and GPOS
        lookup_type: Uint16,
        /// Lookup qualifiers
        lookup_flag: Uint16,
        /// Number of subtables for this lookup
        sub_table_count: Uint16,
        /// Array of offsets to lookup subtables, from beginning of Lookup
        /// table
        #[count(sub_table_count)]
        subtable_offsets: [Offset16],
        /// Index (base 0) into GDEF mark glyph sets structure. This field
        /// is only present if the USE_MARK_FILTERING_SET lookup flag is
        /// set.
        mark_filtering_set: Uint16,
    }
}

toy_table_macro::tables! {
    CoverageFormat1<'a> {
        /// Format identifier — format = 1
        coverage_format: Uint16,
        /// Number of glyphs in the glyph array
        glyph_count: Uint16,
        /// Array of glyph IDs — in numerical order
        #[count(glyph_count)]
        glyph_array: [Uint16],
    }

    CoverageFormat2<'a> {
        /// Format identifier — format = 2
        coverage_format: Uint16,
        /// Number of RangeRecords
        range_count: Uint16,
        /// Array of glyph ranges — ordered by startGlyphID.
        #[count(range_count)]
        range_records: [RangeRecord],
    }

    RangeRecord {
        /// First glyph ID in the range
        start_glyph_i_d: Uint16,
        /// Last glyph ID in the range
        end_glyph_i_d: Uint16,
        /// Coverage Index of first glyph ID in range
        start_coverage_index: Uint16,
    }
}

toy_table_macro::tables! {
    ClassDefFormat1<'a> {
        /// Format identifier — format = 1
        class_format: Uint16,
        /// First glyph ID of the classValueArray
        start_glyph_i_d: Uint16,
        /// Size of the classValueArray
        glyph_count: Uint16,
        /// Array of Class Values — one per glyph ID
        #[count(glyph_count)]
        class_value_array: [Uint16],
    }

    ClassDefFormat2<'a> {
        /// Format identifier — format = 2
        class_format: Uint16,
        /// Number of ClassRangeRecords
        class_range_count: Uint16,
        /// Array of ClassRangeRecords — ordered by startGlyphID
        #[count(class_range_count)]
        class_range_records: [ClassRangeRecord],
    }

    ClassRangeRecord {
        /// First glyph ID in the range
        start_glyph_i_d: Uint16,
        /// Last glyph ID in the range
        end_glyph_i_d: Uint16,
        /// Applied to all glyphs in the range
        class: Uint16,
    }
}

toy_table_macro::tables! {
    SequenceLookupRecord {
        /// Index (zero-based) into the input glyph sequence
        sequence_index: Uint16,
        /// Index (zero-based) into the LookupList
        lookup_list_index: Uint16,
    }

    SequenceContextFormat1<'a> {
        /// Format identifier: format = 1
        format: Uint16,
        /// Offset to Coverage table, from beginning of
        /// SequenceContextFormat1 table
        coverage_offset: Offset16,
        /// Number of SequenceRuleSet tables
        seq_rule_set_count: Uint16,
        /// Array of offsets to SequenceRuleSet tables, from beginning of
        /// SequenceContextFormat1 table (offsets may be NULL)
        #[count(seq_rule_set_count)]
        seq_rule_set_offsets: [Offset16],
    }

    SequenceRuleSet<'a> {
        /// Number of SequenceRule tables
        seq_rule_count: Uint16,
        /// Array of offsets to SequenceRule tables, from beginning of the
        /// SequenceRuleSet table
        #[count(seq_rule_count)]
        seq_rule_offsets: [Offset16],
    }

    SequenceRule<'a> {
        /// Number of glyphs in the input glyph sequence
        glyph_count: Uint16,
        /// Number of SequenceLookupRecords
        seq_lookup_count: Uint16,
        /// - 1]    Array of input glyph IDs—starting with the second glyph
        #[count(glyph_count)]
        input_sequence: [Uint16],
        /// Array of Sequence lookup records
        #[count(seq_lookup_count)]
        seq_lookup_records: [SequenceLookupRecord],
    }

    SequenceContextFormat2<'a> {
        /// Format identifier: format = 2
        format: Uint16,
        /// Offset to Coverage table, from beginning of
        /// SequenceContextFormat2 table
        coverage_offset: Offset16,
        /// Offset to ClassDef table, from beginning of
        /// SequenceContextFormat2 table
        class_def_offset: Offset16,
        /// Number of ClassSequenceRuleSet tables
        class_seq_rule_set_count: Uint16,
        /// Array of offsets to ClassSequenceRuleSet tables, from beginning
        /// of SequenceContextFormat2 table (may be NULL)
        #[count(class_seq_rule_set_count)]
        class_seq_rule_set_offsets: [Offset16],
    }

    ClassSequenceRuleSet<'a> {
        /// Number of ClassSequenceRule tables
        class_seq_rule_count: Uint16,
        /// Array of offsets to ClassSequenceRule tables, from beginning of
        /// ClassSequenceRuleSet table
        #[count(class_seq_rule_count)]
        class_seq_rule_offsets: [Offset16],
    }

    ClassSequenceRule<'a> {
        /// Number of glyphs to be matched
        glyph_count: Uint16,
        /// Number of SequenceLookupRecords
        seq_lookup_count: Uint16,
        /// - 1]    Sequence of classes to be matched to the input glyph
        /// sequence, beginning with the second glyph position
        #[count(glyph_count)]
        input_sequence: [Uint16],
        /// Array of SequenceLookupRecords
        #[count(seq_lookup_count)]
        seq_lookup_records: [SequenceLookupRecord],
    }

    SequenceContextFormat3<'a> {
        /// Format identifier: format = 3
        format: Uint16,
        /// Number of glyphs in the input sequence
        glyph_count: Uint16,
        /// Number of SequenceLookupRecords
        seq_lookup_count: Uint16,
        /// Array of offsets to Coverage tables, from beginning of
        /// SequenceContextFormat3 subtable
        #[count(glyph_count)]
        coverage_offsets: [Offset16],
        /// Array of SequenceLookupRecords
        #[count(seq_lookup_count)]
        seq_lookup_records: [SequenceLookupRecord],
    }

    ChainedSequenceContextFormat1<'a> {
        /// Format identifier: format = 1
        format: Uint16,
        /// Offset to Coverage table, from beginning of
        /// ChainSequenceContextFormat1 table
        coverage_offset: Offset16,
        /// Number of ChainedSequenceRuleSet tables
        chained_seq_rule_set_count: Uint16,
        /// Array of offsets to ChainedSeqRuleSet tables, from beginning of
        /// ChainedSequenceContextFormat1 table (may be NULL)
        #[count(chained_seq_rule_set_count)]
        chained_seq_rule_set_offsets: [Offset16],
    }

    ChainedSequenceRuleSet<'a> {
        /// Number of ChainedSequenceRule tables
        chained_seq_rule_count: Uint16,
        /// Array of offsets to ChainedSequenceRule tables, from beginning
        /// of ChainedSequenceRuleSet table
        #[count(chained_seq_rule_count)]
        chained_seq_rule_offsets: [Offset16],
    }

    ChainedSequenceRule<'a> {
        /// Number of glyphs in the backtrack sequence
        backtrack_glyph_count: Uint16,
        /// Array of backtrack glyph IDs
        #[count(backtrack_glyph_count)]
        backtrack_sequence: [Uint16],
        /// Number of glyphs in the input sequence
        input_glyph_count: Uint16,
        /// - 1]    Array of input glyph IDs—start with second glyph
        #[count(input_glyph_count)]
        input_sequence: [Uint16],
        /// Number of glyphs in the lookahead sequence
        lookahead_glyph_count: Uint16,
        /// Array of lookahead glyph IDs
        #[count(lookahead_glyph_count)]
        lookahead_sequence: [Uint16],
        /// Number of SequenceLookupRecords
        seq_lookup_count: Uint16,
        /// Array of SequenceLookupRecords
        #[count(seq_lookup_count)]
        seq_lookup_records: [SequenceLookupRecord],
    }

    ChainedSequenceContextFormat2<'a> {
        /// Format identifier: format = 2
        format: Uint16,
        /// Offset to Coverage table, from beginning of
        /// ChainedSequenceContextFormat2 table
        coverage_offset: Offset16,
        /// Offset to ClassDef table containing backtrack sequence context,
        /// from beginning of ChainedSequenceContextFormat2 table
        backtrack_class_def_offset: Offset16,
        /// Offset to ClassDef table containing input sequence context,
        /// from beginning of ChainedSequenceContextFormat2 table
        input_class_def_offset: Offset16,
        /// Offset to ClassDef table containing lookahead sequence context,
        /// from beginning of ChainedSequenceContextFormat2 table
        lookahead_class_def_offset: Offset16,
        /// Number of ChainedClassSequenceRuleSet tables
        chained_class_seq_rule_set_count: Uint16,
        /// Array of offsets to ChainedClassSequenceRuleSet tables, from
        /// beginning of ChainedSequenceContextFormat2 table (may be NULL)
        #[count(chained_class_seq_rule_set_count)]
        chained_class_seq_rule_set_offsets: [Offset16],
    }

    ChainedClassSequenceRuleSet<'a> {
        /// Number of ChainedClassSequenceRule tables
        chained_class_seq_rule_count: Uint16,
        /// Array of offsets to ChainedClassSequenceRule tables, from
        /// beginning of ChainedClassSequenceRuleSet
        #[count(chained_class_seq_rule_count)]
        chained_class_seq_rule_offsets: [Offset16],
    }

    ChainedClassSequenceRule<'a> {
        /// Number of glyphs in the backtrack sequence
        backtrack_glyph_count: Uint16,
        /// Array of backtrack-sequence classes
        #[count(backtrack_glyph_count)]
        backtrack_sequence: [Uint16],
        /// Total number of glyphs in the input sequence
        input_glyph_count: Uint16,
        /// - 1]    Array of input sequence classes, beginning with the second
        /// glyph position
        #[count(input_glyph_count)]
        input_sequence: [Uint16],
        /// Number of glyphs in the lookahead sequence
        lookahead_glyph_count: Uint16,
        /// Array of lookahead-sequence classes
        #[count(lookahead_glyph_count)]
        lookahead_sequence: [Uint16],
        /// Number of SequenceLookupRecords
        seq_lookup_count: Uint16,
        /// Array of SequenceLookupRecords
        #[count(seq_lookup_count)]
        seq_lookup_records: [SequenceLookupRecord],
    }

    ChainedSequenceContextFormat3<'a> {
        /// Format identifier: format = 3
        format: Uint16,
        /// Number of glyphs in the backtrack sequence
        backtrack_glyph_count: Uint16,
        /// Array of offsets to coverage tables for the backtrack sequence
        #[count(backtrack_glyph_count)]
        backtrack_coverage_offsets: [Offset16],
        /// Number of glyphs in the input sequence
        input_glyph_count: Uint16,
        /// Array of offsets to coverage tables for the input sequence
        #[count(input_glyph_count)]
        input_coverage_offsets: [Offset16],
        /// Number of glyphs in the lookahead sequence
        lookahead_glyph_count: Uint16,
        /// Array of offsets to coverage tables for the lookahead sequence
        #[count(lookahead_glyph_count)]
        lookahead_coverage_offsets: [Offset16],
        /// Number of SequenceLookupRecords
        seq_lookup_count: Uint16,
        /// Array of SequenceLookupRecords
        #[count(seq_lookup_count)]
        seq_lookup_records: [SequenceLookupRecord],
    }
}

toy_table_macro::tables! {
    //FIXME: I don't know what's going on here. where does the delta count come from?
    //Device<'a> {
    ///// Smallest size to correct, in ppem
    //start_size: Uint16,
    ///// Largest size to correct, in ppem
    //end_size: Uint16,
    ///// Format of deltaValue array data: 0x0001, 0x0002, or 0x0003
    //delta_format: Uint16,
    ///// Array of compressed data
    //#[count(0)]
    //delta_value: [Uint16],
    //}

    VariationIndex {
        /// A delta-set outer index — used to select an item variation
        /// data subtable within the item variation store.
        delta_set_outer_index: Uint16,
        /// A delta-set inner index — used to select a delta-set row
        /// within an item variation data subtable.
        delta_set_inner_index: Uint16,
        /// Format, = 0x8000
        delta_format: Uint16,
    }
}

toy_table_macro::tables! {
    FeatureVariations<'a> {
        /// Major version of the FeatureVariations table — set to 1.
        major_version: Uint16,
        /// Minor version of the FeatureVariations table — set to 0.
        minor_version: Uint16,
        /// Number of feature variation records.
        feature_variation_record_count: Uint32,
        /// Array of feature variation records.
        #[count(feature_variation_record_count)]
        feature_variation_records: [FeatureVariationRecord],
    }

    FeatureVariationRecord {
        /// Offset to a condition set table, from beginning of
        /// FeatureVariations table.
        condition_set_offset: Offset32,
        /// Offset to a feature table substitution table, from beginning of
        /// the FeatureVariations table.
        feature_table_substitution_offset: Offset32,
    }
}

toy_table_macro::tables! {
    ConditionSet<'a> {
        /// Number of conditions for this condition set.
        condition_count: Uint16,
        /// Array of offsets to condition tables, from beginning of the
        /// ConditionSet table.
        #[count(condition_count)]
        condition_offsets: [Offset32],
    }
}

toy_table_macro::tables! {
    ConditionFormat1 {
        /// Format, = 1
        format: Uint16,
        /// Index (zero-based) for the variation axis within the 'fvar'
        /// table.
        axis_index: Uint16,
        /// Minimum value of the font variation instances that satisfy this
        /// condition.
        filter_range_min_value: F2Dot14,
        /// Maximum value of the font variation instances that satisfy this
        /// condition.
        filter_range_max_value: F2Dot14,
    }
}

toy_table_macro::tables! {
    FeatureTableSubstitution<'a> {
        /// Major version of the feature table substitution table — set
        /// to 1
        major_version: Uint16,
        /// Minor version of the feature table substitution table — set
        /// to 0.
        minor_version: Uint16,
        /// Number of feature table substitution records.
        substitution_count: Uint16,
        /// Array of feature table substitution records.
        #[count(substitution_count)]
        substitutions: [FeatureTableSubstitutionRecord],
    }

    FeatureTableSubstitutionRecord {
        /// The feature table index to match.
        feature_index: Uint16,
        /// Offset to an alternate feature table, from start of the
        /// FeatureTableSubstitution table.
        alternate_feature_offset: Offset32,
    }
}
