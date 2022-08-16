#![parse_module(read_fonts::layout::gdef)]

/// [GDEF](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#gdef-header) 1.0
table Gdef {
    /// The major/minor version of the GDEF table
    #[version]
    #[compile(self.compute_version())]
    version: BigEndian<MajorMinor>,
    /// Offset to class definition table for glyph type, from beginning
    /// of GDEF header (may be NULL)
    #[nullable]
    glyph_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to attachment point list table, from beginning of GDEF
    /// header (may be NULL)
    #[nullable]
    attach_list_offset: BigEndian<Offset16<AttachList>>,
    /// Offset to ligature caret list table, from beginning of GDEF
    /// header (may be NULL)
    #[nullable]
    lig_caret_list_offset: BigEndian<Offset16<LigCaretList>>,
    /// Offset to class definition table for mark attachment type, from
    /// beginning of GDEF header (may be NULL)
    #[nullable]
    mark_attach_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to the table of mark glyph set definitions, from
    /// beginning of GDEF header (may be NULL)
    #[available(MajorMinor::VERSION_1_2)]
    #[nullable]
    mark_glyph_sets_def_offset: BigEndian<Offset16<MarkGlyphSets>>,
    /// Offset to the Item Variation Store table, from beginning of
    /// GDEF header (may be NULL)
    #[available(MajorMinor::VERSION_1_3)]
    #[nullable]
    item_var_store_offset: BigEndian<Offset32<ClassDef>>,
}

/// Used in the [Glyph Class Definition Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#glyph-class-definition-table)
enum u16 GlyphClassDef {
    Base = 1,
    Ligature = 2,
    Mark = 3,
    Component = 4,
}

/// [Attachment Point List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#attachment-point-list-table)
table AttachList {
    /// Offset to Coverage table - from beginning of AttachList table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Number of glyphs with attachment points
    #[compile(array_len($attach_point_offsets))]
    glyph_count: BigEndian<u16>,
    /// Array of offsets to AttachPoint tables-from beginning of
    /// AttachList table-in Coverage Index order
    #[count($glyph_count)]
    attach_point_offsets: [BigEndian<Offset16<AttachPoint>>],
}

/// Part of [AttachList]
table AttachPoint {
    /// Number of attachment points on this glyph
    #[compile(array_len($point_indices))]
    point_count: BigEndian<u16>,
    /// Array of contour point indices -in increasing numerical order
    #[count($point_count)]
    point_indices: [BigEndian<u16>],
}

/// [Ligature Caret List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#ligature-caret-list-table)
table LigCaretList {
    /// Offset to Coverage table - from beginning of LigCaretList table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Number of ligature glyphs
    #[compile(array_len($lig_glyph_offsets))]
    lig_glyph_count: BigEndian<u16>,
    /// Array of offsets to LigGlyph tables, from beginning of
    /// LigCaretList table —in Coverage Index order
    #[count($lig_glyph_count)]
    lig_glyph_offsets: [BigEndian<Offset16<LigGlyph>>],
}

/// [Ligature Glyph Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#ligature-glyph-table)
table LigGlyph {
    /// Number of CaretValue tables for this ligature (components - 1)
    #[compile(array_len($caret_value_offsets))]
    caret_count: BigEndian<u16>,
    /// Array of offsets to CaretValue tables, from beginning of
    /// LigGlyph table — in increasing coordinate order
    #[count($caret_count)]
    caret_value_offsets: [BigEndian<Offset16<CaretValue>>],
}

/// [Caret Value Tables](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caret-value-tables)
format u16 CaretValue {
    Format1(CaretValueFormat1),
    Format2(CaretValueFormat2),
    Format3(CaretValueFormat3),
}

/// [CaretValue Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caretvalue-format-1)
table CaretValueFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    caret_value_format: BigEndian<u16>,
    /// X or Y value, in design units
    coordinate: BigEndian<i16>,
}

/// [CaretValue Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caretvalue-format-2)
table CaretValueFormat2 {
    /// Format identifier: format = 2
    #[format = 2]
    caret_value_format: BigEndian<u16>,
    /// Contour point index on glyph
    caret_value_point_index: BigEndian<u16>,
}

/// [CaretValue Format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caretvalue-format-3)
//#[no_compile]
table CaretValueFormat3 {
    /// Format identifier-format = 3
    #[format = 3]
    caret_value_format: BigEndian<u16>,
    /// X or Y value, in design units
    coordinate: BigEndian<i16>,
    /// Offset to Device table (non-variable font) / Variation Index
    /// table (variable font) for X or Y value-from beginning of
    /// CaretValue table
    device_offset: BigEndian<Offset16<Device>>,
}

/// [Mark Glyph Sets Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#mark-glyph-sets-table)
table MarkGlyphSets {
    /// Format identifier == 1
    #[format = 1]
    format: BigEndian<u16>,
    /// Number of mark glyph sets defined
    #[compile(array_len($coverage_offsets))]
    mark_glyph_set_count: BigEndian<u16>,
    /// Array of offsets to mark glyph set coverage tables, from the
    /// start of the MarkGlyphSets table.
    #[count($mark_glyph_set_count)]
    coverage_offsets: [BigEndian<Offset32<CoverageTable>>],
}
