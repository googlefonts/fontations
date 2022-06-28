//! [OpenType™ Layout Common Table Formats](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2)

/// [Script List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#script-list-table-and-script-record)
#[offset_host]
ScriptList<'a> {
    /// Number of ScriptRecords
    #[compute_count(script_records)]
    script_count: BigEndian<u16>,
    /// Array of ScriptRecords, listed alphabetically by script tag
    #[count(script_count)]
    script_records: [ScriptRecord],
}

/// [Script Record](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#script-list-table-and-script-record)
ScriptRecord {
    /// 4-byte script tag identifier
    script_tag: BigEndian<Tag>,
    /// Offset to Script table, from beginning of ScriptList
    script_offset: BigEndian<Offset16<Script>>,
}

/// [Script Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#script-table-and-language-system-record)
#[offset_host]
Script<'a> {
    /// Offset to default LangSys table, from beginning of Script table
    /// — may be NULL
    #[nullable]
    default_lang_sys_offset: BigEndian<Offset16<LangSys>>,
    /// Number of LangSysRecords for this script — excluding the
    /// default LangSys
    #[compute_count(lang_sys_records)]
    lang_sys_count: BigEndian<u16>,
    /// Array of LangSysRecords, listed alphabetically by LangSys tag
    #[count(lang_sys_count)]
    lang_sys_records: [LangSysRecord],
}

LangSysRecord {
    /// 4-byte LangSysTag identifier
    lang_sys_tag: BigEndian<Tag>,
    /// Offset to LangSys table, from beginning of Script table
    lang_sys_offset: BigEndian<Offset16<LangSys>>,
}

/// [Language System Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#language-system-table)
LangSys<'a> {
    /// = NULL (reserved for an offset to a reordering table)
    #[hidden]
    #[compile_type(u16)]
    #[compute(0)]
    lookup_order_offset: BigEndian<Offset16>,
    /// Index of a feature required for this language system; if no
    /// required features = 0xFFFF
    required_feature_index: BigEndian<u16>,
    /// Number of feature index values for this language system —
    /// excludes the required feature
    #[compute_count(feature_indices)]
    feature_index_count: BigEndian<u16>,
    /// Array of indices into the FeatureList, in arbitrary order
    #[count(feature_index_count)]
    feature_indices: [BigEndian<u16>],
}

/// [Feature List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#feature-list-table)
#[offset_host]
FeatureList<'a> {
    /// Number of FeatureRecords in this table
    #[compute_count(feature_records)]
    feature_count: BigEndian<u16>,
    /// Array of FeatureRecords — zero-based (first feature has
    /// FeatureIndex = 0), listed alphabetically by feature tag
    #[count(feature_count)]
    feature_records: [FeatureRecord],
}

/// Part of [FeatureList]
FeatureRecord {
    /// 4-byte feature identification tag
    feature_tag: BigEndian<Tag>,
    /// Offset to Feature table, from beginning of FeatureList
    feature_offset: BigEndian<Offset16<Feature>>,
}

/// [Feature Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#feature-table)
Feature<'a> {
    /// Number of LookupList indices for this feature
    #[compute_count(lookup_list_indices)]
    lookup_index_count: BigEndian<u16>,
    /// Array of indices into the LookupList — zero-based (first
    /// lookup is LookupListIndex = 0)
    #[count(lookup_index_count)]
    lookup_list_indices: [BigEndian<u16>],
}

/// [Lookup List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#lookup-list-table)
#[offset_host]
#[no_compile]
LookupList<'a> {
    /// Number of lookups in this table
    #[compute_count(lookup_offsets)]
    lookup_count: BigEndian<u16>,
    /// Array of offsets to Lookup tables, from beginning of LookupList
    /// — zero based (first lookup is Lookup index = 0)
    #[count(lookup_count)]
    lookup_offsets: [BigEndian<Offset16>],
}

/// [Lookup Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#lookup-table)
#[offset_host]
#[no_compile]
Lookup<'a> {
    /// Different enumerations for GSUB and GPOS
    lookup_type: BigEndian<u16>,
    /// Lookup qualifiers
    lookup_flag: BigEndian<u16>,
    /// Number of subtables for this lookup
    #[compute_count(subtable_offsets)]
    sub_table_count: BigEndian<u16>,
    /// Array of offsets to lookup subtables, from beginning of Lookup
    /// table
    #[count(sub_table_count)]
    subtable_offsets: [BigEndian<Offset16>],
    /// Index (base 0) into GDEF mark glyph sets structure. This field
    /// is only present if the USE_MARK_FILTERING_SET lookup flag is
    /// set.
    mark_filtering_set: BigEndian<u16>,
}

/// [Coverage Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#coverage-format-1)
CoverageFormat1<'a> {
    /// Format identifier — format = 1
    #[compute(1)]
    coverage_format: BigEndian<u16>,
    /// Number of glyphs in the glyph array
    #[compute_count(glyph_array)]
    glyph_count: BigEndian<u16>,
    /// Array of glyph IDs — in numerical order
    #[count(glyph_count)]
    glyph_array: [BigEndian<u16>],
}

/// [Coverage Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#coverage-format-2)
CoverageFormat2<'a> {
    /// Format identifier — format = 2
    #[compute(2)]
    coverage_format: BigEndian<u16>,
    /// Number of RangeRecords
    #[compute_count(range_records)]
    range_count: BigEndian<u16>,
    /// Array of glyph ranges — ordered by startGlyphID.
    #[count(range_count)]
    range_records: [RangeRecord],
}

/// Used in [CoverageFormat2]
RangeRecord {
    /// First glyph ID in the range
    start_glyph_id: BigEndian<u16>,
    /// Last glyph ID in the range
    end_glyph_id: BigEndian<u16>,
    /// Coverage Index of first glyph ID in range
    start_coverage_index: BigEndian<u16>,
}

/// [Coverage Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#coverage-table)
#[format(u16)]
enum CoverageTable<'a> {
    #[version(1)]
    Format1(CoverageFormat1<'a>),
    #[version(2)]
    Format2(CoverageFormat2<'a>),
}

/// [Class Definition Table Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#class-definition-table-format-1)
ClassDefFormat1<'a> {
    /// Format identifier — format = 1
    #[compute(1)]
    class_format: BigEndian<u16>,
    /// First glyph ID of the classValueArray
    start_glyph_id: BigEndian<u16>,
    /// Size of the classValueArray
    #[compute_count(class_value_array)]
    glyph_count: BigEndian<u16>,
    /// Array of Class Values — one per glyph ID
    #[count(glyph_count)]
    class_value_array: [BigEndian<u16>],
}

/// [Class Definition Table Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#class-definition-table-format-2)
ClassDefFormat2<'a> {
    /// Format identifier — format = 2
    #[compute(2)]
    class_format: BigEndian<u16>,
    /// Number of ClassRangeRecords
    #[compute_count(class_range_records)]
    class_range_count: BigEndian<u16>,
    /// Array of ClassRangeRecords — ordered by startGlyphID
    #[count(class_range_count)]
    class_range_records: [ClassRangeRecord],
}

/// Used in [ClassDefFormat2]
ClassRangeRecord {
    /// First glyph ID in the range
    start_glyph_id: BigEndian<u16>,
    /// Last glyph ID in the range
    end_glyph_id: BigEndian<u16>,
    /// Applied to all glyphs in the range
    class: BigEndian<u16>,
}

#[format(u16)]
enum ClassDef<'a> {
    #[version(1)]
    Format1(ClassDefFormat1<'a>),
    #[version(2)]
    Format2(ClassDefFormat2<'a>),
}

/// [Sequence Lookup Record](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-lookup-record)
SequenceLookupRecord {
    /// Index (zero-based) into the input glyph sequence
    sequence_index: BigEndian<u16>,
    /// Index (zero-based) into the LookupList
    lookup_list_index: BigEndian<u16>,
}

/// [Sequence Context Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-context-format-1-simple-glyph-contexts)
#[offset_host]
SequenceContextFormat1<'a> {
    /// Format identifier: format = 1
    #[compute(1)]
    format: BigEndian<u16>,
    /// Offset to Coverage table, from beginning of
    /// SequenceContextFormat1 table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Number of SequenceRuleSet tables
    #[compute_count(seq_rule_set_offsets)]
    seq_rule_set_count: BigEndian<u16>,
    /// Array of offsets to SequenceRuleSet tables, from beginning of
    /// SequenceContextFormat1 table (offsets may be NULL)
    #[count(seq_rule_set_count)]
    #[nullable]
    seq_rule_set_offsets: [BigEndian<Offset16<SequenceRuleSet>>],
}

/// Part of [SequenceContextFormat1]
#[offset_host]
SequenceRuleSet<'a> {
    /// Number of SequenceRule tables
    #[compute_count(seq_rule_offsets)]
    seq_rule_count: BigEndian<u16>,
    /// Array of offsets to SequenceRule tables, from beginning of the
    /// SequenceRuleSet table
    #[count(seq_rule_count)]
    seq_rule_offsets: [BigEndian<Offset16<SequenceRule>>],
}

/// Part of [SequenceContextFormat1]
SequenceRule<'a> {
    /// Number of glyphs in the input glyph sequence
    #[compute(plus_one(self.input_sequence.len()))]
    glyph_count: BigEndian<u16>,
    /// Number of SequenceLookupRecords
    #[compute_count(seq_lookup_records)]
    seq_lookup_count: BigEndian<u16>,
    /// Array of input glyph IDs—starting with the second glyph
    #[count_with(minus_one, glyph_count)]
    input_sequence: [BigEndian<u16>],
    /// Array of Sequence lookup records
    #[count(seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

mod compile {
    fn plus_one(inp: usize) -> u16 {
        inp.saturating_add(1).try_into().unwrap()
    }
}
fn minus_one(inp: u16) -> usize {
    inp.saturating_sub(1) as usize
}

/// [Sequence Context Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-context-format-2-class-based-glyph-contexts)
#[offset_host]
SequenceContextFormat2<'a> {
    /// Format identifier: format = 2
    #[compute(2)]
    format: BigEndian<u16>,
    /// Offset to Coverage table, from beginning of
    /// SequenceContextFormat2 table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Offset to ClassDef table, from beginning of
    /// SequenceContextFormat2 table
    class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Number of ClassSequenceRuleSet tables
    #[compute_count(class_seq_rule_set_offsets)]
    class_seq_rule_set_count: BigEndian<u16>,
    /// Array of offsets to ClassSequenceRuleSet tables, from beginning
    /// of SequenceContextFormat2 table (may be NULL)
    #[count(class_seq_rule_set_count)]
    #[nullable]
    class_seq_rule_set_offsets: [BigEndian<Offset16<ClassSequenceRuleSet>>],
}

/// Part of [SequenceContextFormat2]
#[offset_host]
ClassSequenceRuleSet<'a> {
    /// Number of ClassSequenceRule tables
    #[compute_count(class_seq_rule_offsets)]
    class_seq_rule_count: BigEndian<u16>,
    /// Array of offsets to ClassSequenceRule tables, from beginning of
    /// ClassSequenceRuleSet table
    #[count(class_seq_rule_count)]
    class_seq_rule_offsets: [BigEndian<Offset16<ClassSequenceRule>>],
}

/// Part of [SequenceContextFormat2]
ClassSequenceRule<'a> {
    /// Number of glyphs to be matched
    #[compute(plus_one(self.input_sequence.len()))]
    glyph_count: BigEndian<u16>,
    /// Number of SequenceLookupRecords
    #[compute_count(seq_lookup_records)]
    seq_lookup_count: BigEndian<u16>,
    /// Sequence of classes to be matched to the input glyph sequence,
    /// beginning with the second glyph position
    #[count_with(minus_one, glyph_count)]
    input_sequence: [BigEndian<u16>],
    /// Array of SequenceLookupRecords
    #[count(seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

/// [Sequence Context Format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-context-format-3-coverage-based-glyph-contexts)
#[offset_host]
SequenceContextFormat3<'a> {
    /// Format identifier: format = 3
    #[compute(3)]
    format: BigEndian<u16>,
    /// Number of glyphs in the input sequence
    #[compute_count(coverage_offsets)]
    glyph_count: BigEndian<u16>,
    /// Number of SequenceLookupRecords
    #[compute_count(seq_lookup_records)]
    seq_lookup_count: BigEndian<u16>,
    /// Array of offsets to Coverage tables, from beginning of
    /// SequenceContextFormat3 subtable
    #[count(glyph_count)]
    coverage_offsets: [BigEndian<Offset16<CoverageTable>>],
    /// Array of SequenceLookupRecords
    #[count(seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

#[format(u16)]
#[offset_host]
enum SequenceContext<'a> {
    #[version(1)]
    Format1(SequenceContextFormat1<'a>),
    #[version(2)]
    Format2(SequenceContextFormat2<'a>),
    #[version(3)]
    Format3(SequenceContextFormat3<'a>),
}

/// [Chained Sequence Context Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#chained-sequence-context-format-1-simple-glyph-contexts)
#[offset_host]
ChainedSequenceContextFormat1<'a> {
    /// Format identifier: format = 1
    #[compute(1)]
    format: BigEndian<u16>,
    /// Offset to Coverage table, from beginning of
    /// ChainSequenceContextFormat1 table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Number of ChainedSequenceRuleSet tables
    #[compute_count(chained_seq_rule_set_offsets)]
    chained_seq_rule_set_count: BigEndian<u16>,
    /// Array of offsets to ChainedSeqRuleSet tables, from beginning of
    /// ChainedSequenceContextFormat1 table (may be NULL)
    #[count(chained_seq_rule_set_count)]
    #[nullable]
    chained_seq_rule_set_offsets: [BigEndian<Offset16<ChainedSequenceRuleSet>>],
}

/// Part of [ChainedSequenceContextFormat1]
#[offset_host]
ChainedSequenceRuleSet<'a> {
    /// Number of ChainedSequenceRule tables
    #[compute_count(chained_seq_rule_offsets)]
    chained_seq_rule_count: BigEndian<u16>,
    /// Array of offsets to ChainedSequenceRule tables, from beginning
    /// of ChainedSequenceRuleSet table
    #[count(chained_seq_rule_count)]
    chained_seq_rule_offsets: [BigEndian<Offset16<ChainedSequenceRule>>],
}

/// Part of [ChainedSequenceContextFormat1]
ChainedSequenceRule<'a> {
    /// Number of glyphs in the backtrack sequence
    #[compute_count(backtrack_sequence)]
    backtrack_glyph_count: BigEndian<u16>,
    /// Array of backtrack glyph IDs
    #[count(backtrack_glyph_count)]
    backtrack_sequence: [BigEndian<u16>],
    /// Number of glyphs in the input sequence
    #[compute(plus_one(self.input_sequence.len()))]
    input_glyph_count: BigEndian<u16>,
    /// Array of input glyph IDs—start with second glyph
    #[count_with(minus_one, input_glyph_count)]
    input_sequence: [BigEndian<u16>],
    /// Number of glyphs in the lookahead sequence
    #[compute_count(lookahead_sequence)]
    lookahead_glyph_count: BigEndian<u16>,
    /// Array of lookahead glyph IDs
    #[count(lookahead_glyph_count)]
    lookahead_sequence: [BigEndian<u16>],
    /// Number of SequenceLookupRecords
    #[compute_count(seq_lookup_records)]
    seq_lookup_count: BigEndian<u16>,
    /// Array of SequenceLookupRecords
    #[count(seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

/// [Chained Sequence Context Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#chained-sequence-context-format-2-class-based-glyph-contexts)
#[offset_host]
ChainedSequenceContextFormat2<'a> {
    /// Format identifier: format = 2
    #[compute(2)]
    format: BigEndian<u16>,
    /// Offset to Coverage table, from beginning of
    /// ChainedSequenceContextFormat2 table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Offset to ClassDef table containing backtrack sequence context,
    /// from beginning of ChainedSequenceContextFormat2 table
    backtrack_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to ClassDef table containing input sequence context,
    /// from beginning of ChainedSequenceContextFormat2 table
    input_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to ClassDef table containing lookahead sequence context,
    /// from beginning of ChainedSequenceContextFormat2 table
    lookahead_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Number of ChainedClassSequenceRuleSet tables
    #[compute_count(chained_class_seq_rule_set_offsets)]
    chained_class_seq_rule_set_count: BigEndian<u16>,
    /// Array of offsets to ChainedClassSequenceRuleSet tables, from
    /// beginning of ChainedSequenceContextFormat2 table (may be NULL)
    #[count(chained_class_seq_rule_set_count)]
    #[nullable]
    chained_class_seq_rule_set_offsets: [BigEndian<Offset16<ChainedClassSequenceRuleSet>>],
}

/// Part of [ChainedSequenceContextFormat2]
#[offset_host]
ChainedClassSequenceRuleSet<'a> {
    /// Number of ChainedClassSequenceRule tables
    #[compute_count(chained_class_seq_rule_offsets)]
    chained_class_seq_rule_count: BigEndian<u16>,
    /// Array of offsets to ChainedClassSequenceRule tables, from
    /// beginning of ChainedClassSequenceRuleSet
    #[count(chained_class_seq_rule_count)]
    chained_class_seq_rule_offsets: [BigEndian<Offset16<ChainedClassSequenceRule>>],
}

/// Part of [ChainedSequenceContextFormat2]
ChainedClassSequenceRule<'a> {
    /// Number of glyphs in the backtrack sequence
    #[compute_count(backtrack_sequence)]
    backtrack_glyph_count: BigEndian<u16>,
    /// Array of backtrack-sequence classes
    #[count(backtrack_glyph_count)]
    backtrack_sequence: [BigEndian<u16>],
    /// Total number of glyphs in the input sequence
    #[compute(plus_one(self.input_sequence.len()))]
    input_glyph_count: BigEndian<u16>,
    /// Array of input sequence classes, beginning with the second
    /// glyph position
    #[count_with(minus_one, input_glyph_count)]
    input_sequence: [BigEndian<u16>],
    /// Number of glyphs in the lookahead sequence
    #[compute_count(lookahead_sequence)]
    lookahead_glyph_count: BigEndian<u16>,
    /// Array of lookahead-sequence classes
    #[count(lookahead_glyph_count)]
    lookahead_sequence: [BigEndian<u16>],
    /// Number of SequenceLookupRecords
    #[compute_count(seq_lookup_records)]
    seq_lookup_count: BigEndian<u16>,
    /// Array of SequenceLookupRecords
    #[count(seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

/// [Chained Sequence Context Format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#chained-sequence-context-format-3-coverage-based-glyph-contexts)
#[offset_host]
ChainedSequenceContextFormat3<'a> {
    /// Format identifier: format = 3
    #[compute(3)]
    format: BigEndian<u16>,
    /// Number of glyphs in the backtrack sequence
    #[compute_count(backtrack_coverage_offsets)]
    backtrack_glyph_count: BigEndian<u16>,
    /// Array of offsets to coverage tables for the backtrack sequence
    #[count(backtrack_glyph_count)]
    backtrack_coverage_offsets: [BigEndian<Offset16<CoverageTable>>],
    /// Number of glyphs in the input sequence
    #[compute_count(input_coverage_offsets)]
    input_glyph_count: BigEndian<u16>,
    /// Array of offsets to coverage tables for the input sequence
    #[count(input_glyph_count)]
    input_coverage_offsets: [BigEndian<Offset16<CoverageTable>>],
    /// Number of glyphs in the lookahead sequence
    #[compute_count(lookahead_coverage_offsets)]
    lookahead_glyph_count: BigEndian<u16>,
    /// Array of offsets to coverage tables for the lookahead sequence
    #[count(lookahead_glyph_count)]
    lookahead_coverage_offsets: [BigEndian<Offset16<CoverageTable>>],
    /// Number of SequenceLookupRecords
    #[compute_count(seq_lookup_records)]
    seq_lookup_count: BigEndian<u16>,
    /// Array of SequenceLookupRecords
    #[count(seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

#[format(u16)]
#[offset_host]
enum ChainedSequenceContext<'a> {
    #[version(1)]
    Format1(ChainedSequenceContextFormat1<'a>),
    #[version(2)]
    Format2(ChainedSequenceContextFormat2<'a>),
    #[version(3)]
    Format3(ChainedSequenceContextFormat3<'a>),
}

/// [Device Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#device-and-variationindex-tables)
Device<'a> {
    /// Smallest size to correct, in ppem
    start_size: BigEndian<u16>,
    /// Largest size to correct, in ppem
    end_size: BigEndian<u16>,
    /// Format of deltaValue array data: 0x0001, 0x0002, or 0x0003
    delta_format: BigEndian<u16>,
    /// Array of compressed data
    #[count_all]
    delta_value: [BigEndian<u16>],
}

/// Variation index table
VariationIndex {
    /// A delta-set outer index — used to select an item variation
    /// data subtable within the item variation store.
    delta_set_outer_index: BigEndian<u16>,
    /// A delta-set inner index — used to select a delta-set row
    /// within an item variation data subtable.
    delta_set_inner_index: BigEndian<u16>,
    /// Format, = 0x8000
    delta_format: BigEndian<u16>,
}

/// [FeatureVariations Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#featurevariations-table)
#[offset_host]
FeatureVariations<'a> {
    /// Major version of the FeatureVariations table — set to 1.
    #[compute(1)]
    major_version: BigEndian<u16>,
    /// Minor version of the FeatureVariations table — set to 0.
    #[compute(0)]
    minor_version: BigEndian<u16>,
    /// Number of feature variation records.
    #[compute_count(feature_variation_records)]
    feature_variation_record_count: BigEndian<u32>,
    /// Array of feature variation records.
    #[count(feature_variation_record_count)]
    feature_variation_records: [FeatureVariationRecord],
}

/// Part of [FeatureVariations]
FeatureVariationRecord {
    /// Offset to a condition set table, from beginning of
    /// FeatureVariations table.
    condition_set_offset: BigEndian<Offset32<ConditionSet>>,
    /// Offset to a feature table substitution table, from beginning of
    /// the FeatureVariations table.
    feature_table_substitution_offset: BigEndian<Offset32<FeatureTableSubstitution>>,
}

/// [ConditionSet Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#conditionset-table)
#[offset_host]
ConditionSet<'a> {
    /// Number of conditions for this condition set.
    #[compute_count(condition_offsets)]
    condition_count: BigEndian<u16>,
    /// Array of offsets to condition tables, from beginning of the
    /// ConditionSet table.
    #[count(condition_count)]
    condition_offsets: [BigEndian<Offset32<ConditionFormat1>>],
}

///// [Condition Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#condition-table)
//Condition {
    ///// FIXME: make an enum
    //no_field: fake,
//}

/// [Condition Table Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#condition-table-format-1-font-variation-axis-range): Font Variation Axis Range
ConditionFormat1 {
    /// Format, = 1
    #[compute(1)]
    format: BigEndian<u16>,
    /// Index (zero-based) for the variation axis within the 'fvar'
    /// table.
    axis_index: BigEndian<u16>,
    /// Minimum value of the font variation instances that satisfy this
    /// condition.
    filter_range_min_value: BigEndian<F2Dot14>,
    /// Maximum value of the font variation instances that satisfy this
    /// condition.
    filter_range_max_value: BigEndian<F2Dot14>,
}

/// [FeatureTableSubstitution Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#featuretablesubstitution-table)
#[offset_host]
FeatureTableSubstitution<'a> {
    /// Major version of the feature table substitution table — set
    /// to 1
    #[compute(1)]
    major_version: BigEndian<u16>,
    /// Minor version of the feature table substitution table — set
    /// to 0.
    #[compute(0)]
    minor_version: BigEndian<u16>,
    /// Number of feature table substitution records.
    #[compute_count(substitutions)]
    substitution_count: BigEndian<u16>,
    /// Array of feature table substitution records.
    #[count(substitution_count)]
    substitutions: [FeatureTableSubstitutionRecord],
}

/// Used in [FeatureTableSubstitution]
FeatureTableSubstitutionRecord {
    /// The feature table index to match.
    feature_index: BigEndian<u16>,
    /// Offset to an alternate feature table, from start of the
    /// FeatureTableSubstitution table.
    alternate_feature_offset: BigEndian<Offset32<Feature>>,
}
