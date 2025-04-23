// path (from compile crate) to the generated parse module for this table.
#![parse_module(read_fonts::tables::gpos)]

extern record ValueRecord;

/// [Class Definition Table Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#class-definition-table-format-1)
/// [GPOS Version 1.0](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#gpos-header)
#[tag = "GPOS"]
table Gpos {
    /// The major and minor version of the GPOS table, as a tuple (u16, u16)
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,
    /// Offset to ScriptList table, from beginning of GPOS table
    script_list_offset: Offset16<ScriptList>,
    /// Offset to FeatureList table, from beginning of GPOS table
    feature_list_offset: Offset16<FeatureList>,
    /// Offset to LookupList table, from beginning of GPOS table
    lookup_list_offset: Offset16<PositionLookupList>,
    #[since_version(1.1)]
    #[nullable]
    feature_variations_offset: Offset32<FeatureVariations>,
}

/// A [GPOS Lookup](https://learn.microsoft.com/en-us/typography/opentype/spec/gpos#gsubLookupTypeEnum) subtable.
 group PositionLookup(Lookup, $lookup_type) {
    1 => Single(SinglePos),
    2 => Pair(PairPos),
    3 => Cursive(CursivePosFormat1),
    4 => MarkToBase(MarkBasePosFormat1),
    5 => MarkToLig(MarkLigPosFormat1),
    6 => MarkToMark(MarkMarkPosFormat1),
    7 => Contextual(PositionSequenceContext),
    8 => ChainContextual(PositionChainContext),
    9 => Extension(ExtensionSubtable),
}

/// See [ValueRecord]
flags u16 ValueFormat {
    /// Includes horizontal adjustment for placement
    X_PLACEMENT = 0x0001,
    /// Includes vertical adjustment for placement
    Y_PLACEMENT = 0x0002,
    /// Includes horizontal adjustment for advance
    X_ADVANCE = 0x0004,
    /// Includes vertical adjustment for advance
    Y_ADVANCE = 0x0008,
    /// Includes Device table (non-variable font) / VariationIndex
    /// table (variable font) for horizontal placement
    X_PLACEMENT_DEVICE = 0x0010,
    /// Includes Device table (non-variable font) / VariationIndex
    /// table (variable font) for vertical placement
    Y_PLACEMENT_DEVICE = 0x0020,
    /// Includes Device table (non-variable font) / VariationIndex
    /// table (variable font) for horizontal advance
    X_ADVANCE_DEVICE = 0x0040,
    /// Includes Device table (non-variable font) / VariationIndex
    /// table (variable font) for vertical advance
    Y_ADVANCE_DEVICE = 0x0080,
}

/// [Anchor Tables](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#anchor-tables)
/// position one glyph with respect to another.
format u16 AnchorTable {
    Format1(AnchorFormat1),
    Format2(AnchorFormat2),
    Format3(AnchorFormat3),
}

/// [Anchor Table Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#anchor-table-format-1-design-units): Design Units
table AnchorFormat1 {
    /// Format identifier, = 1
    #[format = 1]
    anchor_format: u16,
    /// Horizontal value, in design units
    x_coordinate: i16,
    /// Vertical value, in design units
    y_coordinate: i16,
}

/// [Anchor Table Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#anchor-table-format-2-design-units-plus-contour-point): Design Units Plus Contour Point
table AnchorFormat2 {
    /// Format identifier, = 2
    #[format = 2]
    anchor_format: u16,
    /// Horizontal value, in design units
    x_coordinate: i16,
    /// Vertical value, in design units
    y_coordinate: i16,
    /// Index to glyph contour point
    anchor_point: u16,
}

/// [Anchor Table Format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#anchor-table-format-3-design-units-plus-device-or-variationindex-tables): Design Units Plus Device or VariationIndex Tables
table AnchorFormat3 {
    /// Format identifier, = 3
    #[format = 3]
    anchor_format: u16,
    /// Horizontal value, in design units
    x_coordinate: i16,
    /// Vertical value, in design units
    y_coordinate: i16,
    /// Offset to Device table (non-variable font) / VariationIndex
    /// table (variable font) for X coordinate, from beginning of
    /// Anchor table (may be NULL)
    #[nullable]
    x_device_offset: Offset16<DeviceOrVariationIndex>,
    /// Offset to Device table (non-variable font) / VariationIndex
    /// table (variable font) for Y coordinate, from beginning of
    /// Anchor table (may be NULL)
    #[nullable]
    y_device_offset: Offset16<DeviceOrVariationIndex>,
}

/// [Mark Array Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-array-table)
table MarkArray {
    /// Number of MarkRecords
    #[compile(array_len($mark_records))]
    mark_count: u16,
    /// Array of MarkRecords, ordered by corresponding glyphs in the
    /// associated mark Coverage table.
    #[count($mark_count)]
    mark_records: [MarkRecord],
}

/// Part of [MarkArray]
record MarkRecord {
    /// Class defined for the associated mark.
    mark_class: u16,
    /// Offset to Anchor table, from beginning of MarkArray table.
    #[offset_from(MarkArray)]
    mark_anchor_offset: Offset16<AnchorTable>,
}

/// [Lookup Type 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-1-single-adjustment-positioning-subtable): Single Adjustment Positioning Subtable
format u16 SinglePos {
    Format1(SinglePosFormat1),
    Format2(SinglePosFormat2),
}

/// [Single Adjustment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#single-adjustment-positioning-format-1-single-positioning-value): Single Positioning Value
table SinglePosFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    pos_format: u16,
    /// Offset to Coverage table, from beginning of SinglePos subtable.
    coverage_offset: Offset16<CoverageTable>,
    /// Defines the types of data in the ValueRecord.
    #[compile(self.compute_value_format())]
    value_format: ValueFormat,
    /// Defines positioning value(s) — applied to all glyphs in the
    /// Coverage table.
    #[read_with($value_format)]
    value_record: ValueRecord,
}

/// [Single Adjustment Positioning Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#single-adjustment-positioning-format-2-array-of-positioning-values): Array of Positioning Values
table SinglePosFormat2 {
    /// Format identifier: format = 2
    #[format = 2]
    pos_format: u16,
    /// Offset to Coverage table, from beginning of SinglePos subtable.
    coverage_offset: Offset16<CoverageTable>,
    /// Defines the types of data in the ValueRecords.
    #[compile(self.compute_value_format())]
    value_format: ValueFormat,
    /// Number of ValueRecords — must equal glyphCount in the
    /// Coverage table.
    #[compile(array_len($value_records))]
    value_count: u16,
    /// Array of ValueRecords — positioning values applied to glyphs.
    #[count($value_count)]
    #[read_with($value_format)]
    value_records: ComputedArray<ValueRecord>,
}

/// [Lookup Type 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-1-single-adjustment-positioning-subtable): Single Adjustment Positioning Subtable
format u16 PairPos {
    Format1(PairPosFormat1),
    Format2(PairPosFormat2),
}

/// [Pair Adjustment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#pair-adjustment-positioning-format-1-adjustments-for-glyph-pairs): Adjustments for Glyph Pairs
table PairPosFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    pos_format: u16,
    /// Offset to Coverage table, from beginning of PairPos subtable.
    coverage_offset: Offset16<CoverageTable>,
    /// Defines the types of data in valueRecord1 — for the first
    /// glyph in the pair (may be zero).
    #[compile(self.compute_value_format1())]
    value_format1: ValueFormat,
    /// Defines the types of data in valueRecord2 — for the second
    /// glyph in the pair (may be zero).
    #[compile(self.compute_value_format2())]
    value_format2: ValueFormat,
    /// Number of PairSet tables
    #[compile(array_len($pair_set_offsets))]
    pair_set_count: u16,
    /// Array of offsets to PairSet tables. Offsets are from beginning
    /// of PairPos subtable, ordered by Coverage Index.
    #[count($pair_set_count)]
    #[read_offset_with($value_format1, $value_format2)]
    #[validate(check_format_consistency)]
    pair_set_offsets: [Offset16<PairSet>],
}

/// Part of [PairPosFormat1]
#[read_args(value_format1: ValueFormat, value_format2: ValueFormat)]
table PairSet {
    /// Number of PairValueRecords
    #[compile(array_len($pair_value_records))]
    pair_value_count: u16,
    /// Array of PairValueRecords, ordered by glyph ID of the second
    /// glyph.
    #[count($pair_value_count)]
    #[read_with($value_format1, $value_format2)]
    pair_value_records: ComputedArray<PairValueRecord>,
}

/// Part of [PairSet]
#[read_args(value_format1: ValueFormat, value_format2: ValueFormat)]
record PairValueRecord {
    /// Glyph ID of second glyph in the pair (first glyph is listed in
    /// the Coverage table).
    second_glyph: GlyphId16,
    /// Positioning data for the first glyph in the pair.
    #[read_with($value_format1)]
    value_record1: ValueRecord,
    /// Positioning data for the second glyph in the pair.
    #[read_with($value_format2)]
    value_record2: ValueRecord,
}

/// [Pair Adjustment Positioning Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#pair-adjustment-positioning-format-2-class-pair-adjustment): Class Pair Adjustment
#[validate(check_length_and_format_conformance)]
table PairPosFormat2 {
    /// Format identifier: format = 2
    #[format = 2]
    pos_format: u16,
    /// Offset to Coverage table, from beginning of PairPos subtable.
    coverage_offset: Offset16<CoverageTable>,
    /// ValueRecord definition — for the first glyph of the pair (may
    /// be zero).
    #[compile(self.compute_value_format1())]
    value_format1: ValueFormat,
    /// ValueRecord definition — for the second glyph of the pair
    /// (may be zero).
    #[compile(self.compute_value_format2())]
    value_format2: ValueFormat,
    /// Offset to ClassDef table, from beginning of PairPos subtable
    /// — for the first glyph of the pair.
    class_def1_offset: Offset16<ClassDef>,
    /// Offset to ClassDef table, from beginning of PairPos subtable
    /// — for the second glyph of the pair.
    class_def2_offset: Offset16<ClassDef>,
    /// Number of classes in classDef1 table — includes Class 0.
    #[compile(self.compute_class1_count())]
    class1_count: u16,
    /// Number of classes in classDef2 table — includes Class 0.
    #[compile(self.compute_class2_count())]
    class2_count: u16,
    /// Array of Class1 records, ordered by classes in classDef1.
    #[read_with($class2_count, $value_format1, $value_format2)]
    #[count($class1_count)]
    class1_records: ComputedArray<Class1Record<'a>>,
}

/// Part of [PairPosFormat2]
#[read_args(class2_count: u16, value_format1: ValueFormat, value_format2: ValueFormat)]
record Class1Record<'a> {
    /// Array of Class2 records, ordered by classes in classDef2.
    #[count($class2_count)]
    #[read_with($value_format1, $value_format2)]
    class2_records: ComputedArray<Class2Record>,
}

/// Part of [PairPosFormat2]
#[read_args(value_format1: ValueFormat, value_format2: ValueFormat)]
record Class2Record {
    /// Positioning for first glyph — empty if valueFormat1 = 0.
    #[read_with($value_format1)]
    value_record1: ValueRecord,
    /// Positioning for second glyph — empty if valueFormat2 = 0.
    #[read_with($value_format2)]
    value_record2: ValueRecord,
}

///// [Lookup Type 3](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-3-cursive-attachment-positioning-subtable): Cursive Attachment Positioning Subtable
//CursivePos {
    ///// //TODO
    //thing: fake,
//}

/// [Cursive Attachment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#cursive-attachment-positioning-format1-cursive-attachment): Cursvie attachment
table CursivePosFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    pos_format: u16,
    /// Offset to Coverage table, from beginning of CursivePos subtable.
    coverage_offset: Offset16<CoverageTable>,
    /// Number of EntryExit records
    #[compile(array_len($entry_exit_record))]
    entry_exit_count: u16,
    /// Array of EntryExit records, in Coverage index order.
    #[count($entry_exit_count)]
    entry_exit_record: [EntryExitRecord],
}

/// Part of [CursivePosFormat1]
record EntryExitRecord {
    /// Offset to entryAnchor table, from beginning of CursivePos
    /// subtable (may be NULL).
    #[nullable]
    #[offset_from(CursivePosFormat1)]
    entry_anchor_offset: Offset16<AnchorTable>,
    /// Offset to exitAnchor table, from beginning of CursivePos
    /// subtable (may be NULL).
    #[nullable]
    #[offset_from(CursivePosFormat1)]
    exit_anchor_offset: Offset16<AnchorTable>,
}

/////// [Lookup Type 4](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-4-mark-to-base-attachment-positioning-subtable): Mark-to-Base Attachment Positioning Subtable
////MarkBasePos {
    /////// //TODO
    ////thing: fake,
////}

/// [Mark-to-Base Attachment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-to-base-attachment-positioning-format-1-mark-to-base-attachment-point): Mark-to-base Attachment Point
table MarkBasePosFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    pos_format: u16,
    /// Offset to markCoverage table, from beginning of MarkBasePos
    /// subtable.
    mark_coverage_offset: Offset16<CoverageTable>,
    /// Offset to baseCoverage table, from beginning of MarkBasePos
    /// subtable.
    base_coverage_offset: Offset16<CoverageTable>,
    /// Number of classes defined for marks
    #[compile(self.compute_mark_class_count())]
    mark_class_count: u16,
    /// Offset to MarkArray table, from beginning of MarkBasePos
    /// subtable.
    mark_array_offset: Offset16<MarkArray>,
    /// Offset to BaseArray table, from beginning of MarkBasePos
    /// subtable.
    #[read_offset_with($mark_class_count)]
    base_array_offset: Offset16<BaseArray>,
}

/// Part of [MarkBasePosFormat1]
#[read_args(mark_class_count: u16)]
table BaseArray {
    /// Number of BaseRecords
    #[compile(array_len($base_records))]
    base_count: u16,
    /// Array of BaseRecords, in order of baseCoverage Index.
    #[count($base_count)]
    #[read_with($mark_class_count)]
    base_records: ComputedArray<BaseRecord<'a>>
}

/// Part of [BaseArray]
#[read_args(mark_class_count: u16)]
record BaseRecord<'a> {
    /// Array of offsets (one per mark class) to Anchor tables. Offsets
    /// are from beginning of BaseArray table, ordered by class
    /// (offsets may be NULL).
    #[nullable]
    #[count($mark_class_count)]
    #[offset_from(BaseArray)]
    base_anchor_offsets: [Offset16<AnchorTable>],
}

/// [Mark-to-Ligature Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-to-ligature-attachment-positioning-format-1-mark-to-ligature-attachment): Mark-to-Ligature Attachment
table MarkLigPosFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    pos_format: u16,
    /// Offset to markCoverage table, from beginning of MarkLigPos
    /// subtable.
    mark_coverage_offset: Offset16<CoverageTable>,
    /// Offset to ligatureCoverage table, from beginning of MarkLigPos
    /// subtable.
    ligature_coverage_offset: Offset16<CoverageTable>,
    /// Number of defined mark classes
    #[compile(self.compute_mark_class_count())]
    mark_class_count: u16,
    /// Offset to MarkArray table, from beginning of MarkLigPos
    /// subtable.
    mark_array_offset: Offset16<MarkArray>,
    /// Offset to LigatureArray table, from beginning of MarkLigPos
    /// subtable.
    #[read_offset_with($mark_class_count)]
    ligature_array_offset: Offset16<LigatureArray>,
}

/// Part of [MarkLigPosFormat1]
#[read_args(mark_class_count: u16)]
table LigatureArray {
    /// Number of LigatureAttach table offsets
    #[compile(array_len($ligature_attach_offsets))]
    ligature_count: u16,
    /// Array of offsets to LigatureAttach tables. Offsets are from
    /// beginning of LigatureArray table, ordered by ligatureCoverage
    /// index.
    #[count($ligature_count)]
    #[read_offset_with($mark_class_count)]
    ligature_attach_offsets: [Offset16<LigatureAttach>],
}

/// Part of [MarkLigPosFormat1]
#[read_args(mark_class_count: u16)]
table LigatureAttach {
    /// Number of ComponentRecords in this ligature
    #[compile(array_len($component_records))]
    component_count: u16,
    /// Array of Component records, ordered in writing direction.
    #[count($component_count)]
    #[read_with($mark_class_count)]
    component_records: ComputedArray<ComponentRecord<'a>>,
}

/// Part of [MarkLigPosFormat1]
#[read_args(mark_class_count: u16)]
record ComponentRecord<'a> {
    /// Array of offsets (one per class) to Anchor tables. Offsets are
    /// from beginning of LigatureAttach table, ordered by class
    /// (offsets may be NULL).
    #[nullable]
    #[count($mark_class_count)]
    #[offset_from(LigatureAttach)]
    ligature_anchor_offsets: [Offset16<AnchorTable>],
}

/// [Mark-to-Mark Attachment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-to-mark-attachment-positioning-format-1-mark-to-mark-attachment): Mark-to-Mark Attachment
table MarkMarkPosFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    pos_format: u16,
    /// Offset to Combining Mark Coverage table, from beginning of
    /// MarkMarkPos subtable.
    mark1_coverage_offset: Offset16<CoverageTable>,
    /// Offset to Base Mark Coverage table, from beginning of
    /// MarkMarkPos subtable.
    mark2_coverage_offset: Offset16<CoverageTable>,
    /// Number of Combining Mark classes defined
    #[compile(self.compute_mark_class_count())]
    mark_class_count: u16,
    /// Offset to MarkArray table for mark1, from beginning of
    /// MarkMarkPos subtable.
    mark1_array_offset: Offset16<MarkArray>,
    /// Offset to Mark2Array table for mark2, from beginning of
    /// MarkMarkPos subtable.
    #[read_offset_with($mark_class_count)]
    mark2_array_offset: Offset16<Mark2Array>,
}

/// Part of [MarkMarkPosFormat1]Class2Record
#[read_args(mark_class_count: u16)]
table Mark2Array {
    /// Number of Mark2 records
    #[compile(array_len($mark2_records))]
    mark2_count: u16,
    /// Array of Mark2Records, in Coverage order.
    #[count($mark2_count)]
    #[read_with($mark_class_count)]
    mark2_records: ComputedArray<Mark2Record<'a>>,
}

/// Part of [MarkMarkPosFormat1]
#[read_args(mark_class_count: u16)]
record Mark2Record<'a> {
    /// Array of offsets (one per class) to Anchor tables. Offsets are
    /// from beginning of Mark2Array table, in class order (offsets may
    /// be NULL).
    #[count($mark_class_count)]
    #[nullable]
    #[offset_from(Mark2Array)]
    mark2_anchor_offsets: [Offset16<AnchorTable>],
}

/// [Extension Positioning Subtable Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#extension-positioning-subtable-format-1)
#[generic_offset(T)]
#[skip_font_write]
table ExtensionPosFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    pos_format: u16,
    /// Lookup type of subtable referenced by extensionOffset (i.e. the
    /// extension subtable).
    extension_lookup_type: u16,
    /// Offset to the extension subtable, of lookup type
    /// extensionLookupType, relative to the start of the
    /// ExtensionPosFormat1 subtable.
    extension_offset: Offset32<T>,
}

/// A [GPOS Extension Positioning](https://learn.microsoft.com/en-us/typography/opentype/spec/gpos#lookuptype-9-extension-positioning) subtable
 group ExtensionSubtable(ExtensionPosFormat1, $extension_lookup_type) {
    1 => Single(SinglePos),
    2 => Pair(PairPos),
    3 => Cursive(CursivePosFormat1),
    4 => MarkToBase(MarkBasePosFormat1),
    5 => MarkToLig(MarkLigPosFormat1),
    6 => MarkToMark(MarkMarkPosFormat1),
    7 => Contextual(PositionSequenceContext),
    8 => ChainContextual(PositionChainContext),
}
