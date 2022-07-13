//use crate::layout::CoverageTable;
//use crate::layout::ClassDef;
//use crate::layout::FeatureList;
//use crate::layout::ScriptList;
//use crate::layout::FeatureVariations;
//use crate::layout::Device;

/// [GPOS Version 1.0](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#gpos-header)
table Gpos {
    /// The major and minor version of the GPOS table, as a tuple (u16, u16)
    version: BigEndian<MajorMinor>,
    /// Offset to ScriptList table, from beginning of GPOS table
    script_list_offset: BigEndian<Offset16<ScriptList>>,
    /// Offset to FeatureList table, from beginning of GPOS table
    feature_list_offset: BigEndian<Offset16<FeatureList>>,
    /// Offset to LookupList table, from beginning of GPOS table
    lookup_list_offset: BigEndian<Offset16<PositionLookupList>>,
    #[available(MajorMinor::VERSION_1_1)]
    #[nullable]
    feature_variations_offset: BigEndian<Offset32<FeatureVariations>>,
}

/// See [ValueRecord]
//#[flags(u16)]
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

format u16 AnchorTable {
    Format1(AnchorFormat1),
    Format2(AnchorFormat2),
    Format3(AnchorFormat3),
}

/// [Anchor Table Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#anchor-table-format-1-design-units): Design Units
//#[format(u16 = 1)]
table AnchorFormat1 {
    /// Format identifier, = 1
    //#[compute(1)]
    #[format = 1]
    anchor_format: BigEndian<u16>,
    /// Horizontal value, in design units
    x_coordinate: BigEndian<i16>,
    /// Vertical value, in design units
    y_coordinate: BigEndian<i16>,
}

/// [Anchor Table Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#anchor-table-format-2-design-units-plus-contour-point): Design Units Plus Contour Point
table AnchorFormat2 {
    /// Format identifier, = 2
    #[format = 2]
    anchor_format: BigEndian<u16>,
    /// Horizontal value, in design units
    x_coordinate: BigEndian<i16>,
    /// Vertical value, in design units
    y_coordinate: BigEndian<i16>,
    /// Index to glyph contour point
    anchor_point: BigEndian<u16>,
}

/// [Anchor Table Format 3]()https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#anchor-table-format-3-design-units-plus-device-or-variationindex-tables: Design Units Plus Device or VariationIndex Tables
table AnchorFormat3 {
    /// Format identifier, = 3
    #[format = 3]
    anchor_format: BigEndian<u16>,
    /// Horizontal value, in design units
    x_coordinate: BigEndian<i16>,
    /// Vertical value, in design units
    y_coordinate: BigEndian<i16>,
    /// Offset to Device table (non-variable font) / VariationIndex
    /// table (variable font) for X coordinate, from beginning of
    /// Anchor table (may be NULL)
    #[nullable]
    x_device_offset: BigEndian<Offset16<Device>>,
    /// Offset to Device table (non-variable font) / VariationIndex
    /// table (variable font) for Y coordinate, from beginning of
    /// Anchor table (may be NULL)
    #[nullable]
    y_device_offset: BigEndian<Offset16<Device>>,
}

/// [Mark Array Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-array-table)
table MarkArray {
    /// Number of MarkRecords
    #[compute_count(mark_records)]
    mark_count: BigEndian<u16>,
    /// Array of MarkRecords, ordered by corresponding glyphs in the
    /// associated mark Coverage table.
    #[count(mark_count)]
    mark_records: [MarkRecord],
}

/// Part of [MarkArray]
record MarkRecord {
    /// Class defined for the associated mark.
    mark_class: BigEndian<u16>,
    /// Offset to Anchor table, from beginning of MarkArray table.
    mark_anchor_offset: BigEndian<Offset16<AnchorTable>>,
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
    pos_format: BigEndian<u16>,
    /// Offset to Coverage table, from beginning of SinglePos subtable.
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Defines the types of data in the ValueRecord.
    value_format: BigEndian<ValueFormat>,
    /// Defines positioning value(s) — applied to all glyphs in the
    /// Coverage table.
    #[no_getter]
    #[len($value_format.record_byte_len())]
    value_record: ValueRecord,
}

/// [Single Adjustment Positioning Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#single-adjustment-positioning-format-2-array-of-positioning-values): Array of Positioning Values
table SinglePosFormat2 {
    /// Format identifier: format = 2
    #[format = 2]
    pos_format: BigEndian<u16>,
    /// Offset to Coverage table, from beginning of SinglePos subtable.
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Defines the types of data in the ValueRecords.
    value_format: BigEndian<ValueFormat>,
    /// Number of ValueRecords — must equal glyphCount in the
    /// Coverage table.
    #[compute_count(value_records)]
    value_count: BigEndian<u16>,
    /// Array of ValueRecords — positioning values applied to glyphs.
    #[no_getter]
    #[len($value_count as usize * $value_format.record_byte_len())]
    value_records: [ValueRecord],
    //#[count_with(value_record_array_len, value_format, value_count)]
    //#[read_with(value_format)]
    //#[compile_type(Vec<ValueRecord>)]
    //value_records: DynSizedArray<'a, ValueFormat, ValueRecord>,
}

///// [Lookup Type 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-1-single-adjustment-positioning-subtable): Single Adjustment Positioning Subtable
//#[format(u16)]
//#[offset_host]
//enum PairPos {
    //#[version(1)]
    //Format1(PairPosFormat1),
    //#[version(2)]
    //Format2(PairPosFormat2),
//}


///// [Pair Adjustment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#pair-adjustment-positioning-format-1-adjustments-for-glyph-pairs): Adjustments for Glyph Pairs
//#[offset_host]
//PairPosFormat1 {
    ///// Format identifier: format = 1
    //#[compute(1)]
    //pos_format: BigEndian<u16>,
    ///// Offset to Coverage table, from beginning of PairPos subtable.
    //coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Defines the types of data in valueRecord1 — for the first
    ///// glyph in the pair (may be zero).
    //value_format1: BigEndian<ValueFormat>,
    ///// Defines the types of data in valueRecord2 — for the second
    ///// glyph in the pair (may be zero).
    //value_format2: BigEndian<ValueFormat>,
    ///// Number of PairSet tables
    //#[compute_count(pair_set_offsets)]
    //pair_set_count: BigEndian<u16>,
    ///// Array of offsets to PairSet tables. Offsets are from beginning
    ///// of PairPos subtable, ordered by Coverage Index.
    //#[count(pair_set_count)]
    //#[to_owned(self.pair_sets_to_owned())]
    //#[read_with(value_format1, value_format2)]
    //pair_set_offsets: [BigEndian<Offset16<PairSet>>],
//}

///// Part of [PairPosFormat1]
//#[read_args(value_format1 = "ValueFormat", value_format2 = "ValueFormat")]
//PairSet {
    ///// Number of PairValueRecords
    //#[compute_count(pair_value_records)]
    //pair_value_count: BigEndian<u16>,
    ///// Array of PairValueRecords, ordered by glyph ID of the second
    ///// glyph.
    //#[count_with(pair_value_record_len, pair_value_count, value_format1, value_format2)]
    //#[read_with(value_format1, value_format2)]
    //#[compile_type(Vec<PairValueRecord>)]
    //pair_value_records: DynSizedArray<'a, (ValueFormat, ValueFormat), PairValueRecord>,
//}

/////// Part of [PairSet]
////#[read_args(value_format1 = "ValueFormat", value_format2 = "ValueFormat")]
////PairValueRecord {
    /////// Glyph ID of second glyph in the pair (first glyph is listed in
    /////// the Coverage table).
    ////second_glyph: BigEndian<u16>,
    /////// Positioning data for the first glyph in the pair.
    ////#[read_with(value_format1)]
    ////value_record1: ValueRecord,
    /////// Positioning data for the second glyph in the pair.
    ////#[read_with(value_format2)]
    ////value_record2: ValueRecord,
////}

///// [Pair Adjustment Positioning Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#pair-adjustment-positioning-format-2-class-pair-adjustment): Class Pair Adjustment
//#[offset_host]
//PairPosFormat2 {
    ///// Format identifier: format = 2
    //#[compute(2)]
    //pos_format: BigEndian<u16>,
    ///// Offset to Coverage table, from beginning of PairPos subtable.
    //coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// ValueRecord definition — for the first glyph of the pair (may
    ///// be zero).
    //value_format1: BigEndian<ValueFormat>,
    ///// ValueRecord definition — for the second glyph of the pair
    ///// (may be zero).
    //value_format2: BigEndian<ValueFormat>,
    ///// Offset to ClassDef table, from beginning of PairPos subtable
    ///// — for the first glyph of the pair.
    //class_def1_offset: BigEndian<Offset16<ClassDef>>,
    ///// Offset to ClassDef table, from beginning of PairPos subtable
    ///// — for the second glyph of the pair.
    //class_def2_offset: BigEndian<Offset16<ClassDef>>,
    ///// Number of classes in classDef1 table — includes Class 0.
    //#[compute(self.class_def1_offset.get().unwrap().class_count())]
    //class1_count: BigEndian<u16>,
    ///// Number of classes in classDef2 table — includes Class 0.
    //#[compute(self.class_def2_offset.get().unwrap().class_count())]
    //class2_count: BigEndian<u16>,
    ///// Array of Class1 records, ordered by classes in classDef1.
    //#[count_with(class1_record_len, class1_count, class2_count, value_format1, value_format2)]
    //#[read_with(class2_count, value_format1, value_format2)]
    //#[compile_type(Vec<Class1Record>)]
    //class1_records: DynSizedArray<'a, (u16, ValueFormat, ValueFormat), Class1Record>,
//}

///// Part of [PairPosFormat2]
//#[read_args(class2_count = "u16", value_format1 = "ValueFormat", value_format2 = "ValueFormat")]
//Class1Record {
    ///// Array of Class2 records, ordered by classes in classDef2.
    //#[count_with(class2_record_len, class2_count, value_format1, value_format2)]
    //#[read_with(value_format1, value_format2)]
    //#[compile_type(Vec<Class2Record>)]
    //class2_records: DynSizedArray<'a, (ValueFormat, ValueFormat), Class2Record>,
//}

///// Part of [PairPosFormat2]
//#[read_args(value_format1 = "ValueFormat", value_format2 = "ValueFormat")]
//Class2Record {
    ///// Positioning for first glyph — empty if valueFormat1 = 0.
    //#[read_with(value_format1)]
    //value_record1: ValueRecord,
    ///// Positioning for second glyph — empty if valueFormat2 = 0.
    //#[read_with(value_format2)]
    //value_record2: ValueRecord,
//}

/////// [Lookup Type 3](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-3-cursive-attachment-positioning-subtable): Cursive Attachment Positioning Subtable
////CursivePos {
    /////// //TODO
    ////thing: fake,
////}

///// [Cursive Attachment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#cursive-attachment-positioning-format1-cursive-attachment): Cursvie attachment
//#[offset_host]
//CursivePosFormat1 {
    ///// Format identifier: format = 1
    //#[compute(1)]
    //pos_format: BigEndian<u16>,
    ///// Offset to Coverage table, from beginning of CursivePos subtable.
    //coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Number of EntryExit records
    //#[compute_count(entry_exit_record)]
    //entry_exit_count: BigEndian<u16>,
    ///// Array of EntryExit records, in Coverage index order.
    //#[count(entry_exit_count)]
    //entry_exit_record: [EntryExitRecord],
//}

///// Part of [CursivePosFormat1]
//EntryExitRecord {
    ///// Offset to entryAnchor table, from beginning of CursivePos
    ///// subtable (may be NULL).
    //#[nullable]
    //entry_anchor_offset: BigEndian<Offset16<AnchorTable>>,
    ///// Offset to exitAnchor table, from beginning of CursivePos
    ///// subtable (may be NULL).
    //#[nullable]
    //exit_anchor_offset: BigEndian<Offset16<AnchorTable>>,
//}

/////// [Lookup Type 4](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-4-mark-to-base-attachment-positioning-subtable): Mark-to-Base Attachment Positioning Subtable
////MarkBasePos {
    /////// //TODO
    ////thing: fake,
////}

///// [Mark-to-Base Attachment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-to-base-attachment-positioning-format-1-mark-to-base-attachment-point): Mark-to-base Attachment Point
//#[offset_host]
//MarkBasePosFormat1 {
    ///// Format identifier: format = 1
    //#[compute(1)]
    //pos_format: BigEndian<u16>,
    ///// Offset to markCoverage table, from beginning of MarkBasePos
    ///// subtable.
    //mark_coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Offset to baseCoverage table, from beginning of MarkBasePos
    ///// subtable.
    //base_coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Number of classes defined for marks
    //#[compute(self.mark_array_offset.get().unwrap().class_count())]
    //mark_class_count: BigEndian<u16>,
    ///// Offset to MarkArray table, from beginning of MarkBasePos
    ///// subtable.
    //mark_array_offset: BigEndian<Offset16<MarkArray>>,
    ///// Offset to BaseArray table, from beginning of MarkBasePos
    ///// subtable.
    //#[to_owned(self.base_array_to_owned())]
    //#[read_with(mark_class_count)]
    //base_array_offset: BigEndian<Offset16<BaseArray>>,
//}

///// Part of [MarkBasePosFormat1]
//#[read_args(mark_class_count = "u16")]
//#[offset_host]
//BaseArray {
    ///// Number of BaseRecords
    //#[compute_count(base_records)]
    //base_count: BigEndian<u16>,
    ///// Array of BaseRecords, in order of baseCoverage Index.
    //#[count_with(nested_offset_array_len, base_count, mark_class_count)]
    //#[read_with(mark_class_count)]
    //#[compile_type(Vec<BaseRecord>)]
    //base_records: DynSizedArray<'a, u16, BaseRecord>,
//}

///// Part of [BaseArray]
//#[read_args(mark_class_count = "u16")]
//BaseRecord {
    ///// Array of offsets (one per mark class) to Anchor tables. Offsets
    ///// are from beginning of BaseArray table, ordered by class
    ///// (offsets may be NULL).
    //#[count(mark_class_count)]
    //#[nullable]
    //base_anchor_offsets: [BigEndian<Offset16<AnchorTable>>],
//}

///// [Mark-to-Ligature Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-to-ligature-attachment-positioning-format-1-mark-to-ligature-attachment): Mark-to-Ligature Attachment
//#[offset_host]
//MarkLigPosFormat1 {
    ///// Format identifier: format = 1
    //#[compute(1)]
    //pos_format: BigEndian<u16>,
    ///// Offset to markCoverage table, from beginning of MarkLigPos
    ///// subtable.
    //mark_coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Offset to ligatureCoverage table, from beginning of MarkLigPos
    ///// subtable.
    //ligature_coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Number of defined mark classes
    //#[compute(self.mark_array_offset.get().unwrap().class_count())]
    //mark_class_count: BigEndian<u16>,
    ///// Offset to MarkArray table, from beginning of MarkLigPos
    ///// subtable.
    //mark_array_offset: BigEndian<Offset16<MarkArray>>,
    ///// Offset to LigatureArray table, from beginning of MarkLigPos
    ///// subtable.
    //#[to_owned(self.ligature_array_to_owned())]
    //#[read_with(mark_class_count)]
    //ligature_array_offset: BigEndian<Offset16<LigatureArray>>,
//}

///// Part of [MarkLigPosFormat1]
//#[read_args(mark_class_count = "u16")]
//#[offset_host]
//#[skip_to_owned]
//LigatureArray {
    ///// Number of LigatureAttach table offsets
    //#[compute_count(ligature_attach_offsets)]
    //ligature_count: BigEndian<u16>,
    ///// Array of offsets to LigatureAttach tables. Offsets are from
    ///// beginning of LigatureArray table, ordered by ligatureCoverage
    ///// index.
    //#[count(ligature_count)]
    //#[skip_offset_getter]
    //ligature_attach_offsets: [BigEndian<Offset16<LigatureAttach>>],
//}

///// Part of [MarkLigPosFormat1]
//#[offset_host]
//#[read_args(mark_class_count = "u16")]
//LigatureAttach {
    ///// Number of ComponentRecords in this ligature
    //#[compute_count(component_records)]
    //component_count: BigEndian<u16>,
    ///// Array of Component records, ordered in writing direction.
    //#[count_with(nested_offset_array_len, component_count, mark_class_count)]
    //#[read_with(mark_class_count)]
    //#[compile_type(Vec<ComponentRecord>)]
    //component_records: DynSizedArray<'a, u16, ComponentRecord>,
//}

///// Part of [MarkLigPosFormat1]
//#[read_args(mark_class_count = "u16")]
//ComponentRecord {
    ///// Array of offsets (one per class) to Anchor tables. Offsets are
    ///// from beginning of LigatureAttach table, ordered by class
    ///// (offsets may be NULL).
    //#[count(mark_class_count)]
    //#[nullable]
    //ligature_anchor_offsets: [BigEndian<Offset16<AnchorTable>>],
//}

///// [Mark-to-Mark Attachment Positioning Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#mark-to-mark-attachment-positioning-format-1-mark-to-mark-attachment): Mark-to-Mark Attachment
//#[offset_host]
//MarkMarkPosFormat1 {
    ///// Format identifier: format = 1
    //#[compute(1)]
    //pos_format: BigEndian<u16>,
    ///// Offset to Combining Mark Coverage table, from beginning of
    ///// MarkMarkPos subtable.
    //mark1_coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Offset to Base Mark Coverage table, from beginning of
    ///// MarkMarkPos subtable.
    //mark2_coverage_offset: BigEndian<Offset16<CoverageTable>>,
    ///// Number of Combining Mark classes defined
    //#[compute(self.mark1_array_offset.get().unwrap().class_count())]
    //mark_class_count: BigEndian<u16>,
    ///// Offset to MarkArray table for mark1, from beginning of
    ///// MarkMarkPos subtable.
    //mark1_array_offset: BigEndian<Offset16<MarkArray>>,
    ///// Offset to Mark2Array table for mark2, from beginning of
    ///// MarkMarkPos subtable.
    //#[to_owned(self.mark2_array_to_owned())]
    //#[read_with(mark_class_count)]
    //mark2_array_offset: BigEndian<Offset16<Mark2Array>>,
//}

///// Part of [MarkMarkPosFormat1]Class2Record
//#[read_args(mark_class_count = "u16")]
//#[offset_host]
//Mark2Array {
    ///// Number of Mark2 records
    //#[compute_count(mark2_records)]
    //mark2_count: BigEndian<u16>,
    ///// Array of Mark2Records, in Coverage order.
    //#[count_with(nested_offset_array_len, mark2_count, mark_class_count)]
    //#[read_with(mark_class_count)]
    //#[compile_type(Vec<Mark2Record>)]
    //mark2_records: DynSizedArray<'a, u16, Mark2Record>,
//}

///// Part of [MarkMarkPosFormat1]
//#[read_args(mark_class_count = "u16")]
//Mark2Record {
    ///// Array of offsets (one per class) to Anchor tables. Offsets are
    ///// from beginning of Mark2Array table, in class order (offsets may
    ///// be NULL).
    //#[count(mark_class_count)]
    //#[nullable]
    //mark2_anchor_offsets: [BigEndian<Offset16<AnchorTable>>],
//}

///// [Extension Positioning Subtable Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#extension-positioning-subtable-format-1)
//#[no_compile]
//#[offset_host]
//ExtensionPosFormat1 {
    ///// Format identifier: format = 1
    //pos_format: BigEndian<u16>,
    ///// Lookup type of subtable referenced by extensionOffset (i.e. the
    ///// extension subtable).
    //extension_lookup_type: BigEndian<u16>,
    ///// Offset to the extension subtable, of lookup type
    ///// extensionLookupType, relative to the start of the
    ///// ExtensionPosFormat1 subtable.
    //extension_offset: BigEndian<Offset32>,
//}
