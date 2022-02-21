use crate::{MajorMinor, Offset16};

toy_table_macro::tables! {
    Gpos1_0 {
        /// Major version of the GPOS table, = 1
        major_version: Uint16,
        /// Minor version of the GPOS table, = 0
        minor_version: Uint16,
        /// Offset to ScriptList table, from beginning of GPOS table
        script_list_offset: Offset16,
        /// Offset to FeatureList table, from beginning of GPOS table
        feature_list_offset: Offset16,
        /// Offset to LookupList table, from beginning of GPOS table
        lookup_list_offset: Offset16,
    }

    Gpos1_1 {
        /// Major version of the GPOS table, = 1
        major_version: Uint16,
        /// Minor version of the GPOS table, = 1
        minor_version: Uint16,
        /// Offset to ScriptList table, from beginning of GPOS table
        script_list_offset: Offset16,
        /// Offset to FeatureList table, from beginning of GPOS table
        feature_list_offset: Offset16,
        /// Offset to LookupList table, from beginning of GPOS table
        lookup_list_offset: Offset16,
        /// Offset to FeatureVariations table, from beginning of GPOS table
        /// (may be NULL)
        feature_variations_offset: Offset32,
    }

    #[format(MajorMinor)]
    enum Gpos {
        #[version(Gpos::VERSION_1_0)]
        Version1_0(Gpos1_0),
        #[version(Gpos::VERSION_1_1)]
        Version1_1(Gpos1_1),
    }
}

impl Gpos {
    const VERSION_1_0: MajorMinor = MajorMinor::new(1, 0);
    const VERSION_1_1: MajorMinor = MajorMinor::new(1, 1);
}

toy_table_macro::tables! {
    ValueRecord {
        /// Horizontal adjustment for placement, in design units.
        x_placement: Int16,
        /// Vertical adjustment for placement, in design units.
        y_placement: Int16,
        /// Horizontal adjustment for advance, in design units — only
        /// used for horizontal layout.
        x_advance: Int16,
        /// Vertical adjustment for advance, in design units — only used
        /// for vertical layout.
        y_advance: Int16,
        /// Offset to Device table (non-variable font) / VariationIndex
        /// table (variable font) for horizontal placement, from beginning
        /// of the immediate parent table (SinglePos or PairPosFormat2
        /// lookup subtable, PairSet table within a PairPosFormat1 lookup
        /// subtable) — may be NULL.
        x_pla_device_offset: Offset16,
        /// Offset to Device table (non-variable font) / VariationIndex
        /// table (variable font) for vertical placement, from beginning of
        /// the immediate parent table (SinglePos or PairPosFormat2 lookup
        /// subtable, PairSet table within a PairPosFormat1 lookup
        /// subtable) — may be NULL.
        y_pla_device_offset: Offset16,
        /// Offset to Device table (non-variable font) / VariationIndex
        /// table (variable font) for horizontal advance, from beginning of
        /// the immediate parent table (SinglePos or PairPosFormat2 lookup
        /// subtable, PairSet table within a PairPosFormat1 lookup
        /// subtable) — may be NULL.
        x_adv_device_offset: Offset16,
        /// Offset to Device table (non-variable font) / VariationIndex
        /// table (variable font) for vertical advance, from beginning of
        /// the immediate parent table (SinglePos or PairPosFormat2 lookup
        /// subtable, PairSet table within a PairPosFormat1 lookup
        /// subtable) — may be NULL.
        y_adv_device_offset: Offset16,
    }

    AnchorFormat1 {
        /// Format identifier, = 1
        anchor_format: Uint16,
        /// Horizontal value, in design units
        x_coordinate: Int16,
        /// Vertical value, in design units
        y_coordinate: Int16,
    }

    AnchorFormat2 {
        /// Format identifier, = 2
        anchor_format: Uint16,
        /// Horizontal value, in design units
        x_coordinate: Int16,
        /// Vertical value, in design units
        y_coordinate: Int16,
        /// Index to glyph contour point
        anchor_point: Uint16,
    }

    AnchorFormat3 {
        /// Format identifier, = 3
        anchor_format: Uint16,
        /// Horizontal value, in design units
        x_coordinate: Int16,
        /// Vertical value, in design units
        y_coordinate: Int16,
        /// Offset to Device table (non-variable font) / VariationIndex
        /// table (variable font) for X coordinate, from beginning of
        /// Anchor table (may be NULL)
        x_device_offset: Offset16,
        /// Offset to Device table (non-variable font) / VariationIndex
        /// table (variable font) for Y coordinate, from beginning of
        /// Anchor table (may be NULL)
        y_device_offset: Offset16,
    }

    MarkArray<'a> {
        /// Number of MarkRecords
        mark_count: Uint16,
        /// Array of MarkRecords, ordered by corresponding glyphs in the
        /// associated mark Coverage table.
        #[count(mark_count)]
        mark_records: [MarkRecord],
    }

    MarkRecord {
        /// Class defined for the associated mark.
        mark_class: Uint16,
        /// Offset to Anchor table, from beginning of MarkArray table.
        mark_anchor_offset: Offset16,
    }
}

toy_table_macro::tables! {
    SinglePos1 {
        /// Format identifier: format = 1
        pos_format: Uint16,
        /// Offset to Coverage table, from beginning of SinglePos subtable.
        coverage_offset: Offset16,
        /// Defines the types of data in the ValueRecord.
        value_format: Uint16,
        /// Defines positioning value(s) — applied to all glyphs in the
        /// Coverage table.
        value_record: ValueRecord,
    }

    SinglePos2<'a> {
        /// Format identifier: format = 2
        pos_format: Uint16,
        /// Offset to Coverage table, from beginning of SinglePos subtable.
        coverage_offset: Offset16,
        /// Defines the types of data in the ValueRecords.
        value_format: Uint16,
        /// Number of ValueRecords — must equal glyphCount in the
        /// Coverage table.
        value_count: Uint16,
        /// Array of ValueRecords — positioning values applied to glyphs.
        #[count(value_count)]
        value_records: [ValueRecord],
    }
}

toy_table_macro::tables! {
    PairPos1<'a> {
        /// Format identifier: format = 1
        pos_format: Uint16,
        /// Offset to Coverage table, from beginning of PairPos subtable.
        coverage_offset: Offset16,
        /// Defines the types of data in valueRecord1 — for the first
        /// glyph in the pair (may be zero).
        value_format1: Uint16,
        /// Defines the types of data in valueRecord2 — for the second
        /// glyph in the pair (may be zero).
        value_format2: Uint16,
        /// Number of PairSet tables
        pair_set_count: Uint16,
        /// Array of offsets to PairSet tables. Offsets are from beginning
        /// of PairPos subtable, ordered by Coverage Index.
        #[count(pair_set_count)]
        pair_set_offsets: [Offset16],
    }

    PairSet<'a> {
        /// Number of PairValueRecords
        pair_value_count: Uint16,
        /// Array of PairValueRecords, ordered by glyph ID of the second
        /// glyph.
        #[count(pair_value_count)]
        pair_value_records: [PairValueRecord],
    }

    PairValueRecord {
        /// Glyph ID of second glyph in the pair (first glyph is listed in
        /// the Coverage table).
        second_glyph: Uint16,
        /// Positioning data for the first glyph in the pair.
        value_record1: ValueRecord,
        /// Positioning data for the second glyph in the pair.
        value_record2: ValueRecord,
    }

    PairPos2<'a> {
        /// Format identifier: format = 2
        pos_format: Uint16,
        /// Offset to Coverage table, from beginning of PairPos subtable.
        coverage_offset: Offset16,
        /// ValueRecord definition — for the first glyph of the pair (may
        /// be zero).
        value_format1: Uint16,
        /// ValueRecord definition — for the second glyph of the pair
        /// (may be zero).
        value_format2: Uint16,
        /// Offset to ClassDef table, from beginning of PairPos subtable
        /// — for the first glyph of the pair.
        class_def1_offset: Offset16,
        /// Offset to ClassDef table, from beginning of PairPos subtable
        /// — for the second glyph of the pair.
        class_def2_offset: Offset16,
        /// Number of classes in classDef1 table — includes Class 0.
        class1_count: Uint16,
        /// Number of classes in classDef2 table — includes Class 0.
        class2_count: Uint16,
        /// Array of Class1 records, ordered by classes in classDef1.
        #[count(class1_count)]
        #[inner_count(class2_count)]
        //FIXME: this is not implemented
        class1_records: [Class1Record<'a>],
        //class1_records: [Class2Record], // this is just so that we continue to compile
    }

    Class1Record<'a> {
        /// Array of Class2 records, ordered by classes in classDef2.
        #[count(class2_count)]
        class2_records: [Class2Record],
    }

    Class2Record {
        /// Positioning for first glyph — empty if valueFormat1 = 0.
        value_record1: ValueRecord,
        /// Positioning for second glyph — empty if valueFormat2 = 0.
        value_record2: ValueRecord,
    }
}

// Cursive position
toy_table_macro::tables! {
    CursivePos1<'a> {
        /// Format identifier: format = 1
        pos_format: Uint16,
        /// Offset to Coverage table, from beginning of CursivePos subtable.
        coverage_offset: Offset16,
        /// Number of EntryExit records
        entry_exit_count: Uint16,
        /// Array of EntryExit records, in Coverage index order.
        #[count(entry_exit_count)]
        entry_exit_record: [EntryExitRecord],
    }

    EntryExitRecord {
        /// Offset to entryAnchor table, from beginning of CursivePos
        /// subtable (may be NULL).
        entry_anchor_offset: Offset16,
        /// Offset to exitAnchor table, from beginning of CursivePos
        /// subtable (may be NULL).
        exit_anchor_offset: Offset16,
    }
}

//mark-base positioning
toy_table_macro::tables! {
    MarkBasePos1 {
        /// Format identifier: format = 1
        pos_format: Uint16,
        /// Offset to markCoverage table, from beginning of MarkBasePos
        /// subtable.
        mark_coverage_offset: Offset16,
        /// Offset to baseCoverage table, from beginning of MarkBasePos
        /// subtable.
        base_coverage_offset: Offset16,
        /// Number of classes defined for marks
        mark_class_count: Uint16,
        /// Offset to MarkArray table, from beginning of MarkBasePos
        /// subtable.
        mark_array_offset: Offset16,
        /// Offset to BaseArray table, from beginning of MarkBasePos
        /// subtable.
        base_array_offset: Offset16,
    }

    BaseArray<'a> {
        /// Number of BaseRecords
        base_count: Uint16,
        /// Array of BaseRecords, in order of baseCoverage Index.
        #[count(base_count)]
        base_records: [BaseRecord],
    }

    BaseRecord<'a> {
        /// Array of offsets (one per mark class) to Anchor tables. Offsets
        /// are from beginning of BaseArray table, ordered by class
        /// (offsets may be NULL).
        #[count(mark_class_count)]
        base_anchor_offsets: [Offset16],
    }
}

// mark-lig positioning
toy_table_macro::tables! {
    MarkLigPos1 {
        /// Format identifier: format = 1
        pos_format: Uint16,
        /// Offset to markCoverage table, from beginning of MarkLigPos
        /// subtable.
        mark_coverage_offset: Offset16,
        /// Offset to ligatureCoverage table, from beginning of MarkLigPos
        /// subtable.
        ligature_coverage_offset: Offset16,
        /// Number of defined mark classes
        mark_class_count: Uint16,
        #[hidden]
        reserved_padding: Uint32,
        /// Offset to MarkArray table, from beginning of MarkLigPos
        /// subtable.
        mark_array_offset: Offset16,
        /// Offset to LigatureArray table, from beginning of MarkLigPos
        /// subtable.
        ligature_array_offset: Offset16,
    }

    LigatureArray<'a> {
        /// Number of LigatureAttach table offsets
        ligature_count: Uint16,
        /// Array of offsets to LigatureAttach tables. Offsets are from
        /// beginning of LigatureArray table, ordered by ligatureCoverage
        /// index.
        #[count(ligature_count)]
        ligature_attach_offsets: [Offset16],
    }

    LigatureAttach<'a> {
        /// Number of ComponentRecords in this ligature
        component_count: Uint16,
        /// Array of Component records, ordered in writing direction.
        #[count(component_count)]
        component_records: [ComponentRecord],
    }

    ComponentRecord<'a> {
        /// Array of offsets (one per class) to Anchor tables. Offsets are
        /// from beginning of LigatureAttach table, ordered by class
        /// (offsets may be NULL).
        #[count(mark_class_count)]
        ligature_anchor_offsets: [Offset16],
    }
}

// mark-to-mark
toy_table_macro::tables! {
    MarkMarkPosFormat1 {
        /// Format identifier: format = 1
        pos_format: Uint16,
        /// Offset to Combining Mark Coverage table, from beginning of
        /// MarkMarkPos subtable.
        mark1_coverage_offset: Offset16,
        /// Offset to Base Mark Coverage table, from beginning of
        /// MarkMarkPos subtable.
        mark2_coverage_offset: Offset16,
        /// Number of Combining Mark classes defined
        mark_class_count: Uint16,
        /// Offset to MarkArray table for mark1, from beginning of
        /// MarkMarkPos subtable.
        mark1_array_offset: Offset16,
        /// Offset to Mark2Array table for mark2, from beginning of
        /// MarkMarkPos subtable.
        mark2_array_offset: Offset16,
    }

    Mark2Array<'a> {
        /// Number of Mark2 records
        mark2_count: Uint16,
        /// Array of Mark2Records, in Coverage order.
        #[count(mark2_count)]
        mark2_records: [Mark2Record],
    }

    Mark2Record<'a> {
        /// Array of offsets (one per class) to Anchor tables. Offsets are
        /// from beginning of Mark2Array table, in class order (offsets may
        /// be NULL).
        #[count(mark_class_count)]
        mark2_anchor_offsets: [Offset16],
    }
}

toy_table_macro::tables! {
    ExtensionPosFormat1 {
        /// Format identifier: format = 1
        pos_format: Uint16,
        /// Lookup type of subtable referenced by extensionOffset (i.e. the
        /// extension subtable).
        extension_lookup_type: Uint16,
        /// Offset to the extension subtable, of lookup type
        /// extensionLookupType, relative to the start of the
        /// ExtensionPosFormat1 subtable.
        extension_offset: Offset32,
    }
}
