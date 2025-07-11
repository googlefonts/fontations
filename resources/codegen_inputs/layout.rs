// path (from compile crate) to the generated parse module for this table.
#![parse_module(read_fonts::tables::layout)]

extern scalar LookupFlag;

/// [Script List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#script-list-table-and-script-record)
table ScriptList {
    /// Number of ScriptRecords
    #[compile(array_len($script_records))]
    script_count: u16,
    /// Array of ScriptRecords, listed alphabetically by script tag
    #[count($script_count)]
    script_records: [ScriptRecord],
}

/// [Script Record](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#script-list-table-and-script-record)
record ScriptRecord {
    /// 4-byte script tag identifier
    script_tag: Tag,
    /// Offset to Script table, from beginning of ScriptList
    #[offset_from(ScriptList)]
    script_offset: Offset16<Script>,
}

/// [Script Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#script-table-and-language-system-record)
table Script {
    /// Offset to default LangSys table, from beginning of Script table
    /// — may be NULL
    #[nullable]
    default_lang_sys_offset: Offset16<LangSys>,
    /// Number of LangSysRecords for this script — excluding the
    /// default LangSys
    #[compile(array_len($lang_sys_records))]
    lang_sys_count: u16,
    /// Array of LangSysRecords, listed alphabetically by LangSys tag
    #[count($lang_sys_count)]
    lang_sys_records: [LangSysRecord],
}

record LangSysRecord {
    /// 4-byte LangSysTag identifier
    lang_sys_tag: Tag,
    /// Offset to LangSys table, from beginning of Script table
    #[offset_from(Script)]
    lang_sys_offset: Offset16<LangSys>,
}

/// [Language System Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#language-system-table)
table LangSys {
    /// = NULL (reserved for an offset to a reordering table)
    #[skip_getter]
    #[compile(0)]
    lookup_order_offset: u16,
    /// Index of a feature required for this language system; if no
    /// required features = 0xFFFF
    #[default(0xFFFF)]
    required_feature_index: u16,
    /// Number of feature index values for this language system —
    /// excludes the required feature
    #[compile(array_len($feature_indices))]
    feature_index_count: u16,
    /// Array of indices into the FeatureList, in arbitrary order
    #[count($feature_index_count)]
    feature_indices: [u16],
}

/// [Feature List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#feature-list-table)
table FeatureList {
    /// Number of FeatureRecords in this table
    #[compile(array_len($feature_records))]
    feature_count: u16,
    /// Array of FeatureRecords — zero-based (first feature has
    /// FeatureIndex = 0), listed alphabetically by feature tag
    #[count($feature_count)]
    feature_records: [FeatureRecord],
}

/// Part of [FeatureList]
record FeatureRecord {
    /// 4-byte feature identification tag
    feature_tag: Tag,
    /// Offset to Feature table, from beginning of FeatureList
    #[read_offset_with($feature_tag)]
    #[offset_from(FeatureList)]
    feature_offset: Offset16<Feature>,
}

/// [Feature Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#feature-table)
#[read_args(feature_tag: Tag)]
table Feature {
    /// Offset from start of Feature table to FeatureParams table, if defined for the feature and present, else NULL
    #[nullable]
    #[read_offset_with($feature_tag)]
    feature_params_offset: Offset16<FeatureParams>,
    /// Number of LookupList indices for this feature
    #[compile(array_len($lookup_list_indices))]
    lookup_index_count: u16,
    /// Array of indices into the LookupList — zero-based (first
    /// lookup is LookupListIndex = 0)
    #[count($lookup_index_count)]
    lookup_list_indices: [u16],
}

/// [Lookup List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#lookup-list-table)
#[generic_offset(T)]
table LookupList {
    /// Number of lookups in this table
    #[compile(array_len($lookup_offsets))]
    lookup_count: u16,
    /// Array of offsets to Lookup tables, from beginning of LookupList
    /// — zero based (first lookup is Lookup index = 0)
    #[count($lookup_count)]
    lookup_offsets: [Offset16<T>],
}

/// [Lookup Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#lookup-table)
#[generic_offset(T)]
#[skip_font_write]
table Lookup {
    /// Different enumerations for GSUB and GPOS
    #[compile(skip)]
    lookup_type: u16,
    /// Lookup qualifiers
    #[traverse_with(traverse_lookup_flag)]
    lookup_flag: LookupFlag,
    /// Number of subtables for this lookup
    #[compile(array_len($subtable_offsets))]
    sub_table_count: u16,
    /// Array of offsets to lookup subtables, from beginning of Lookup
    /// table
    #[count($sub_table_count)]
    subtable_offsets: [Offset16<T>],
    /// Index (base 0) into GDEF mark glyph sets structure. This field
    /// is only present if the USE_MARK_FILTERING_SET lookup flag is
    /// set.
    #[if_flag($lookup_flag, LookupFlag::USE_MARK_FILTERING_SET)]
    mark_filtering_set: u16,
}

/// [Coverage Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#coverage-format-1)
table CoverageFormat1 {
    /// Format identifier — format = 1
    #[format = 1]
    coverage_format: u16,
    /// Number of glyphs in the glyph array
    #[compile(array_len($glyph_array))]
    glyph_count: u16,
    /// Array of glyph IDs — in numerical order
    #[count($glyph_count)]
    glyph_array: [GlyphId16],
}

/// [Coverage Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#coverage-format-2)
table CoverageFormat2 {
    /// Format identifier — format = 2
    #[format = 2]
    coverage_format: u16,
    /// Number of RangeRecords
    #[compile(array_len($range_records))]
    range_count: u16,
    /// Array of glyph ranges — ordered by startGlyphID.
    #[count($range_count)]
    range_records: [RangeRecord],
}

/// Used in [CoverageFormat2]
record RangeRecord {
    /// First glyph ID in the range
    start_glyph_id: GlyphId16,
    /// Last glyph ID in the range
    end_glyph_id: GlyphId16,
    /// Coverage Index of first glyph ID in range
    start_coverage_index: u16,
}

/// [Coverage Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#coverage-table)
format u16 CoverageTable {
    Format1(CoverageFormat1),
    Format2(CoverageFormat2),
}

/// [Class Definition Table Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#class-definition-table-format-1)
table ClassDefFormat1 {
    /// Format identifier — format = 1
    #[format = 1]
    class_format: u16,
    /// First glyph ID of the classValueArray
    start_glyph_id: GlyphId16,
    /// Size of the classValueArray
    #[compile(array_len($class_value_array))]
    glyph_count: u16,
    /// Array of Class Values — one per glyph ID
    #[count($glyph_count)]
    class_value_array: [u16],
}

/// [Class Definition Table Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#class-definition-table-format-2)
table ClassDefFormat2 {
    /// Format identifier — format = 2
    #[format = 2]
    class_format: u16,
    /// Number of ClassRangeRecords
    #[compile(array_len($class_range_records))]
    class_range_count: u16,
    /// Array of ClassRangeRecords — ordered by startGlyphID
    #[count($class_range_count)]
    class_range_records: [ClassRangeRecord],
}

/// Used in [ClassDefFormat2]
record ClassRangeRecord {
    /// First glyph ID in the range
    #[validate(validate_glyph_range)]
    start_glyph_id: GlyphId16,
    /// Last glyph ID in the range
    end_glyph_id: GlyphId16,
    /// Applied to all glyphs in the range
    class: u16,
}

/// A [Class Definition Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#class-definition-table)
format u16 ClassDef {
    Format1(ClassDefFormat1),
    Format2(ClassDefFormat2),
}

/// [Sequence Lookup Record](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-lookup-record)
record SequenceLookupRecord {
    /// Index (zero-based) into the input glyph sequence
    sequence_index: u16,
    /// Index (zero-based) into the LookupList
    lookup_list_index: u16,
}

/// [Sequence Context Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-context-format-1-simple-glyph-contexts)
table SequenceContextFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    format: u16,
    /// Offset to Coverage table, from beginning of
    /// SequenceContextFormat1 table
    coverage_offset: Offset16<CoverageTable>,
    /// Number of SequenceRuleSet tables
    #[compile(array_len($seq_rule_set_offsets))]
    seq_rule_set_count: u16,
    /// Array of offsets to SequenceRuleSet tables, from beginning of
    /// SequenceContextFormat1 table (offsets may be NULL)
    #[count($seq_rule_set_count)]
    #[nullable]
    seq_rule_set_offsets: [Offset16<SequenceRuleSet>],
}

/// Part of [SequenceContextFormat1]
table SequenceRuleSet {
    /// Number of SequenceRule tables
    #[compile(array_len($seq_rule_offsets))]
    seq_rule_count: u16,
    /// Array of offsets to SequenceRule tables, from beginning of the
    /// SequenceRuleSet table
    #[count($seq_rule_count)]
    seq_rule_offsets: [Offset16<SequenceRule>],
}

/// Part of [SequenceContextFormat1]
table SequenceRule {
    /// Number of glyphs in the input glyph sequence
    #[compile(plus_one($input_sequence.len()))]
    glyph_count: u16,
    /// Number of SequenceLookupRecords
    #[compile(array_len($seq_lookup_records))]
    seq_lookup_count: u16,
    /// Array of input glyph IDs—starting with the second glyph
    #[count(subtract($glyph_count, 1))]
    input_sequence: [GlyphId16],
    /// Array of Sequence lookup records
    #[count($seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

/// [Sequence Context Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-context-format-2-class-based-glyph-contexts)
table SequenceContextFormat2 {
    /// Format identifier: format = 2
    #[format = 2]
    format: u16,
    /// Offset to Coverage table, from beginning of
    /// SequenceContextFormat2 table
    coverage_offset: Offset16<CoverageTable>,
    /// Offset to ClassDef table, from beginning of
    /// SequenceContextFormat2 table
    class_def_offset: Offset16<ClassDef>,
    /// Number of ClassSequenceRuleSet tables
    #[compile(array_len($class_seq_rule_set_offsets))]
    class_seq_rule_set_count: u16,
    /// Array of offsets to ClassSequenceRuleSet tables, from beginning
    /// of SequenceContextFormat2 table (may be NULL)
    #[count($class_seq_rule_set_count)]
    #[nullable]
    class_seq_rule_set_offsets: [Offset16<ClassSequenceRuleSet>],
}

/// Part of [SequenceContextFormat2]
table ClassSequenceRuleSet {
    /// Number of ClassSequenceRule tables
    #[compile(array_len($class_seq_rule_offsets))]
    class_seq_rule_count: u16,
    /// Array of offsets to ClassSequenceRule tables, from beginning of
    /// ClassSequenceRuleSet table
    #[count($class_seq_rule_count)]
    class_seq_rule_offsets: [Offset16<ClassSequenceRule>],
}

/// Part of [SequenceContextFormat2]
table ClassSequenceRule {
    /// Number of glyphs to be matched
    #[compile(plus_one($input_sequence.len()))]
    glyph_count: u16,
    /// Number of SequenceLookupRecords
    #[compile(array_len($seq_lookup_records))]
    seq_lookup_count: u16,
    /// Sequence of classes to be matched to the input glyph sequence,
    /// beginning with the second glyph position
    #[count(subtract($glyph_count, 1))]
    input_sequence: [u16],
    /// Array of SequenceLookupRecords
    #[count($seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

/// [Sequence Context Format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#sequence-context-format-3-coverage-based-glyph-contexts)
table SequenceContextFormat3 {
    /// Format identifier: format = 3
    #[format = 3]
    format: u16,
    /// Number of glyphs in the input sequence
    #[compile(array_len($coverage_offsets))]
    glyph_count: u16,
    /// Number of SequenceLookupRecords
    #[compile(array_len($seq_lookup_records))]
    seq_lookup_count: u16,
    /// Array of offsets to Coverage tables, from beginning of
    /// SequenceContextFormat3 subtable
    #[count($glyph_count)]
    coverage_offsets: [Offset16<CoverageTable>],
    /// Array of SequenceLookupRecords
    #[count($seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

format u16 SequenceContext {
    Format1(SequenceContextFormat1),
    Format2(SequenceContextFormat2),
    Format3(SequenceContextFormat3),
}

/// [Chained Sequence Context Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#chained-sequence-context-format-1-simple-glyph-contexts)
table ChainedSequenceContextFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    format: u16,
    /// Offset to Coverage table, from beginning of
    /// ChainSequenceContextFormat1 table
    coverage_offset: Offset16<CoverageTable>,
    /// Number of ChainedSequenceRuleSet tables
    #[compile(array_len($chained_seq_rule_set_offsets))]
    chained_seq_rule_set_count: u16,
    /// Array of offsets to ChainedSeqRuleSet tables, from beginning of
    /// ChainedSequenceContextFormat1 table (may be NULL)
    #[count($chained_seq_rule_set_count)]
    #[nullable]
    chained_seq_rule_set_offsets: [Offset16<ChainedSequenceRuleSet>],
}

/// Part of [ChainedSequenceContextFormat1]
table ChainedSequenceRuleSet {
    /// Number of ChainedSequenceRule tables
    #[compile(array_len($chained_seq_rule_offsets))]
    chained_seq_rule_count: u16,
    /// Array of offsets to ChainedSequenceRule tables, from beginning
    /// of ChainedSequenceRuleSet table
    #[count($chained_seq_rule_count)]
    chained_seq_rule_offsets: [Offset16<ChainedSequenceRule>],
}

/// Part of [ChainedSequenceContextFormat1]
table ChainedSequenceRule {
    /// Number of glyphs in the backtrack sequence
    #[compile(array_len($backtrack_sequence))]
    backtrack_glyph_count: u16,
    /// Array of backtrack glyph IDs
    #[count($backtrack_glyph_count)]
    backtrack_sequence: [GlyphId16],
    /// Number of glyphs in the input sequence
    #[compile(plus_one($input_sequence.len()))]
    input_glyph_count: u16,
    /// Array of input glyph IDs—start with second glyph
    #[count(subtract($input_glyph_count, 1))]
    input_sequence: [GlyphId16],
    /// Number of glyphs in the lookahead sequence
    #[compile(array_len($lookahead_sequence))]
    lookahead_glyph_count: u16,
    /// Array of lookahead glyph IDs
    #[count($lookahead_glyph_count)]
    lookahead_sequence: [GlyphId16],
    /// Number of SequenceLookupRecords
    #[compile(array_len($seq_lookup_records))]
    seq_lookup_count: u16,
    /// Array of SequenceLookupRecords
    #[count($seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

/// [Chained Sequence Context Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#chained-sequence-context-format-2-class-based-glyph-contexts)
table ChainedSequenceContextFormat2 {
    /// Format identifier: format = 2
    #[format = 2]
    format: u16,
    /// Offset to Coverage table, from beginning of
    /// ChainedSequenceContextFormat2 table
    coverage_offset: Offset16<CoverageTable>,
    /// Offset to ClassDef table containing backtrack sequence context,
    /// from beginning of ChainedSequenceContextFormat2 table
    backtrack_class_def_offset: Offset16<ClassDef>,
    /// Offset to ClassDef table containing input sequence context,
    /// from beginning of ChainedSequenceContextFormat2 table
    input_class_def_offset: Offset16<ClassDef>,
    /// Offset to ClassDef table containing lookahead sequence context,
    /// from beginning of ChainedSequenceContextFormat2 table
    lookahead_class_def_offset: Offset16<ClassDef>,
    /// Number of ChainedClassSequenceRuleSet tables
    #[compile(array_len($chained_class_seq_rule_set_offsets))]
    chained_class_seq_rule_set_count: u16,
    /// Array of offsets to ChainedClassSequenceRuleSet tables, from
    /// beginning of ChainedSequenceContextFormat2 table (may be NULL)
    #[count($chained_class_seq_rule_set_count)]
    #[nullable]
    chained_class_seq_rule_set_offsets: [Offset16<ChainedClassSequenceRuleSet>],
}

/// Part of [ChainedSequenceContextFormat2]
table ChainedClassSequenceRuleSet {
    /// Number of ChainedClassSequenceRule tables
    #[compile(array_len($chained_class_seq_rule_offsets))]
    chained_class_seq_rule_count: u16,
    /// Array of offsets to ChainedClassSequenceRule tables, from
    /// beginning of ChainedClassSequenceRuleSet
    #[count($chained_class_seq_rule_count)]
    chained_class_seq_rule_offsets: [Offset16<ChainedClassSequenceRule>],
}

/// Part of [ChainedSequenceContextFormat2]
table ChainedClassSequenceRule {
    /// Number of glyphs in the backtrack sequence
    #[compile(array_len($backtrack_sequence))]
    backtrack_glyph_count: u16,
    /// Array of backtrack-sequence classes
    #[count($backtrack_glyph_count)]
    backtrack_sequence: [u16],
    /// Total number of glyphs in the input sequence
    #[compile(plus_one($input_sequence.len()))]
    input_glyph_count: u16,
    /// Array of input sequence classes, beginning with the second
    /// glyph position
    #[count(subtract($input_glyph_count, 1))]
    input_sequence: [u16],
    /// Number of glyphs in the lookahead sequence
    #[compile(array_len($lookahead_sequence))]
    lookahead_glyph_count: u16,
    /// Array of lookahead-sequence classes
    #[count($lookahead_glyph_count)]
    lookahead_sequence: [u16],
    /// Number of SequenceLookupRecords
    #[compile(array_len($seq_lookup_records))]
    seq_lookup_count: u16,
    /// Array of SequenceLookupRecords
    #[count($seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

/// [Chained Sequence Context Format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#chained-sequence-context-format-3-coverage-based-glyph-contexts)
table ChainedSequenceContextFormat3 {
    /// Format identifier: format = 3
    #[format = 3]
    format: u16,
    /// Number of glyphs in the backtrack sequence
    #[compile(array_len($backtrack_coverage_offsets))]
    backtrack_glyph_count: u16,
    /// Array of offsets to coverage tables for the backtrack sequence
    #[count($backtrack_glyph_count)]
    backtrack_coverage_offsets: [Offset16<CoverageTable>],
    /// Number of glyphs in the input sequence
    #[compile(array_len($input_coverage_offsets))]
    input_glyph_count: u16,
    /// Array of offsets to coverage tables for the input sequence
    #[count($input_glyph_count)]
    input_coverage_offsets: [Offset16<CoverageTable>],
    /// Number of glyphs in the lookahead sequence
    #[compile(array_len($lookahead_coverage_offsets))]
    lookahead_glyph_count: u16,
    /// Array of offsets to coverage tables for the lookahead sequence
    #[count($lookahead_glyph_count)]
    lookahead_coverage_offsets: [Offset16<CoverageTable>],
    /// Number of SequenceLookupRecords
    #[compile(array_len($seq_lookup_records))]
    seq_lookup_count: u16,
    /// Array of SequenceLookupRecords
    #[count($seq_lookup_count)]
    seq_lookup_records: [SequenceLookupRecord],
}

format u16 ChainedSequenceContext {
    Format1(ChainedSequenceContextFormat1),
    Format2(ChainedSequenceContextFormat2),
    Format3(ChainedSequenceContextFormat3),
}

/// [Device](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#device-and-variationindex-tables)
/// delta formats
enum u16 DeltaFormat {
    /// Signed 2-bit value, 8 values per uint16
    #[default]
    Local2BitDeltas = 0x0001,
    /// Signed 4-bit value, 4 values per uint16
    Local4BitDeltas = 0x0002,
    /// Signed 8-bit value, 2 values per uint16
    Local8BitDeltas = 0x0003,
    /// VariationIndex table, contains a delta-set index pair.
    VariationIndex = 0x8000,
//0x7FFC	Reserved	For future use — set to 0
}

/// [Device Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#device-and-variationindex-tables)
#[skip_constructor]
table Device {
    /// Smallest size to correct, in ppem
    start_size: u16,
    /// Largest size to correct, in ppem
    end_size: u16,
    /// Format of deltaValue array data: 0x0001, 0x0002, or 0x0003
    delta_format: DeltaFormat,
    /// Array of compressed data
    #[count(delta_value_count($delta_format, $start_size, $end_size))]
    delta_value: [u16],
}

/// Variation index table
table VariationIndex {
    /// A delta-set outer index — used to select an item variation
    /// data subtable within the item variation store.
    delta_set_outer_index: u16,
    /// A delta-set inner index — used to select a delta-set row
    /// within an item variation data subtable.
    delta_set_inner_index: u16,
    /// Format, = 0x8000
    #[compile(DeltaFormat::VariationIndex)]
    delta_format: DeltaFormat,
}

/// A type representing a temporary identifier for a set of variation deltas.
///
/// The final indices used in the VariationIndex table are not known until
/// all deltas have been collected. This variant is used to assign a
/// temporary identifier during compilation.
///
/// This type is not part of the spec and will never appear in an actual font file.
/// It is intended to serve as a sentinel value, and will panic when written,
/// ensuring that all VariationIndex tables have been correctly mapped before
/// the font is compiled.
#[write_fonts_only]
#[skip_from_obj]
#[skip_font_write]
table PendingVariationIndex {
    /// A unique identifier for a given set of deltas.
    delta_set_id: u32,
}

/// Either a [Device] table (in a non-variable font) or a [VariationIndex] table (in a variable font)
format DeltaFormat@4 DeviceOrVariationIndex {
    #[match_if($format != DeltaFormat::VariationIndex)]
    Device(Device),
    #[match_if($format == DeltaFormat::VariationIndex)]
    VariationIndex(VariationIndex),
    #[write_fonts_only]
    PendingVariationIndex(PendingVariationIndex),
}

/// [FeatureVariations Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#featurevariations-table)
table FeatureVariations {
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Number of feature variation records.
    #[compile(array_len($feature_variation_records))]
    feature_variation_record_count: u32,
    /// Array of feature variation records.
    #[count($feature_variation_record_count)]
    feature_variation_records: [FeatureVariationRecord],
}

/// Part of [FeatureVariations]
record FeatureVariationRecord {
    /// Offset to a condition set table, from beginning of
    /// FeatureVariations table.
    #[nullable]
    #[offset_from(FeatureVariations)]
    condition_set_offset: Offset32<ConditionSet>,
    /// Offset to a feature table substitution table, from beginning of
    /// the FeatureVariations table.
    #[nullable]
    #[offset_from(FeatureVariations)]
    feature_table_substitution_offset: Offset32<FeatureTableSubstitution>,
}

/// [ConditionSet Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#conditionset-table)
table ConditionSet {
    /// Number of conditions for this condition set.
    #[compile(array_len($condition_offsets))]
    condition_count: u16,
    /// Array of offsets to condition tables, from beginning of the
    /// ConditionSet table.
    #[count($condition_count)]
    condition_offsets: [Offset32<Condition>],
}

/// [Condition Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#condition-table)
/// 
/// Formats 2..5 are implementations of specification changes currently under debate at ISO for an OFF
/// update. For the time being the specification is <https://github.com/harfbuzz/boring-expansion-spec/blob/main/ConditionTree.md>.
format u16 Condition {
    Format1AxisRange(ConditionFormat1),
    Format2VariableValue(ConditionFormat2),
    Format3And(ConditionFormat3),
    Format4Or(ConditionFormat4),
    Format5Negate(ConditionFormat5),
}

/// [Condition Table Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#condition-table-format-1-font-variation-axis-range): Font Variation Axis Range
table ConditionFormat1 {
    /// Format, = 1
    #[format = 1]
    format: u16,
    /// Index (zero-based) for the variation axis within the 'fvar'
    /// table.
    axis_index: u16,
    /// Minimum value of the font variation instances that satisfy this
    /// condition.
    filter_range_min_value: F2Dot14,
    /// Maximum value of the font variation instances that satisfy this
    /// condition.
    filter_range_max_value: F2Dot14,
}

/// [Condition Table Format 2](https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otData.py#L3237-L3255): Variation index
table ConditionFormat2 {
    /// Format, = 2
    #[format = 2]
    format: u16,
    /// Value at default instance.
    default_value: i16,
    /// Variation index to vary the value based on current designspace location.
    var_index: u32,
}

/// [Condition Table Format 3](https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otData.py#L3257-L3275): AND
table ConditionFormat3 {
    /// Format, = 3
    #[format = 3]
    format: u16,
    /// Number of conditions.
    condition_count: u8,
    /// Array of condition tables for this conjunction (AND) expression.
    #[count($condition_count)]
    condition_offsets: [Offset24<Condition>],
}

/// [Condition Table Format 4](https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otData.py#L3276-L3295): OR
table ConditionFormat4 {
    /// Format, = 4
    #[format = 4]
    format: u16,
    /// Number of conditions.
    condition_count: u8,
    /// Array of condition tables for this disjunction (OR) expression.
    #[count($condition_count)]
    condition_offsets: [Offset24<Condition>],
}

/// [Condition Table Format 5](https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otData.py#L3296-L3308): NOT
table ConditionFormat5 {
    /// Format, = 5
    #[format = 5]
    format: u16,
    /// Condition to negate.
    condition_offset: Offset24<Condition>,
}

/// [FeatureTableSubstitution Table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#featuretablesubstitution-table)
table FeatureTableSubstitution {
    /// Major & minor version of the table: (1, 0)
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Number of feature table substitution records.
    #[compile(array_len($substitutions))]
    substitution_count: u16,
    /// Array of feature table substitution records.
    #[count($substitution_count)]
    substitutions: [FeatureTableSubstitutionRecord],
}

/// Used in [FeatureTableSubstitution]
record FeatureTableSubstitutionRecord {
    /// The feature table index to match.
    feature_index: u16,
    /// Offset to an alternate feature table, from start of the
    /// FeatureTableSubstitution table.
    #[offset_getter(alternate_feature)] // custom impl, we need to pass a fake tag
    alternate_feature_offset: Offset32<Feature>,
}

table SizeParams {
    /// The first value represents the design size in 720/inch units (decipoints).
    ///
    /// The design size entry must be non-zero. When there is a design size but
    /// no recommended size range, the rest of the array will consist of zeros.
    design_size: u16,
    /// The second value has no independent meaning, but serves as an identifier that associates fonts in a subfamily.
    ///
    /// All fonts which share a Typographic or Font Family name and which differ
    /// only by size range shall have the same subfamily value, and no fonts
    /// which differ in weight or style shall have the same subfamily value.
    /// If this value is zero, the remaining fields in the array will be ignored.
    identifier: u16,
    /// The third value enables applications to use a single name for the subfamily identified by the second value.
    ///
    /// If the preceding value is non-zero, this value must be set in the range
    /// 256 – 32767 (inclusive). It records the value of a field in the 'name'
    /// table, which must contain English-language strings encoded in Windows
    /// Unicode and Macintosh Roman, and may contain additional strings localized
    /// to other scripts and languages. Each of these strings is the name
    /// an application should use, in combination with the family name, to
    /// represent the subfamily in a menu. Applications will choose the
    /// appropriate version based on their selection criteria.
    name_entry: u16,
    /// The fourth and fifth values represent the small end of the recommended
    /// usage range (exclusive) and the large end of the recommended usage range
    /// (inclusive), stored in 720/inch units (decipoints).
    ///
    /// Ranges must not overlap, and should generally be contiguous.
    range_start: u16,
    range_end: u16,
}

table StylisticSetParams {
    //#[version]
    #[compile(0)]
    version: u16,
    /// The 'name' table name ID that specifies a string (or strings, for
    /// multiple languages) for a user-interface label for this feature.
    ///
    /// The value of uiLabelNameId is expected to be in the font-specific name
    /// ID range (256-32767), though that is not a requirement in this Feature
    /// Parameters specification. The user-interface label for the feature can
    /// be provided in multiple languages. An English string should be included
    /// as a fallback. The string should be kept to a minimal length to fit
    /// comfortably with different application interfaces.
    ui_name_id: NameId,
}

/// featureParams for ['cv01'-'cv99'](https://docs.microsoft.com/en-us/typography/opentype/spec/features_ae#cv01-cv99)
table CharacterVariantParams {
    /// Format number is set to 0.
    #[format = 0]
    format: u16,
    /// The 'name' table name ID that specifies a string (or strings,
    /// for multiple languages) for a user-interface label for this
    /// feature. (May be NULL.)
    feat_ui_label_name_id: NameId,
    /// The 'name' table name ID that specifies a string (or strings,
    /// for multiple languages) that an application can use for tooltip
    /// text for this feature. (May be NULL.)
    feat_ui_tooltip_text_name_id: NameId,
    /// The 'name' table name ID that specifies sample text that
    /// illustrates the effect of this feature. (May be NULL.)
    sample_text_name_id: NameId,
    /// Number of named parameters. (May be zero.)
    num_named_parameters: u16,
    /// The first 'name' table name ID used to specify strings for
    /// user-interface labels for the feature parameters. (Must be zero
    /// if numParameters is zero.)
    first_param_ui_label_name_id: NameId,
    /// The count of characters for which this feature provides glyph
    /// variants. (May be zero.)
    #[compile(array_len($character))]
    char_count: u16,
    /// The Unicode Scalar Value of the characters for which this
    /// feature provides glyph variants.
    #[count($char_count)]
    character: [Uint24],
}

