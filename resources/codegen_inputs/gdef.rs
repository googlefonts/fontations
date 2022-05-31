/// [GDEF](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#gdef-header) 1.0
#[offset_host]
Gdef1_0<'a> {
    /// Major version of the GDEF table, = 1
    major_version: BigEndian<u16>,
    /// Minor version of the GDEF table, = 0
    minor_version: BigEndian<u16>,
    /// Offset to class definition table for glyph type, from beginning
    /// of GDEF header (may be NULL)
    glyph_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to attachment point list table, from beginning of GDEF
    /// header (may be NULL)
    attach_list_offset: BigEndian<Offset16<AttachList>>,
    /// Offset to ligature caret list table, from beginning of GDEF
    /// header (may be NULL)
    lig_caret_list_offset: BigEndian<Offset16<LigCaretList>>,
    /// Offset to class definition table for mark attachment type, from
    /// beginning of GDEF header (may be NULL)
    mark_attach_class_def_offset: BigEndian<Offset16<ClassDef>>,
}

/// [GDEF](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#gdef-header) 1.2
#[offset_host]
Gdef1_2<'a> {
    /// Major version of the GDEF table, = 1
    major_version: BigEndian<u16>,
    /// Minor version of the GDEF table, = 2
    minor_version: BigEndian<u16>,
    /// Offset to class definition table for glyph type, from beginning
    /// of GDEF header (may be NULL)
    glyph_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to attachment point list table, from beginning of GDEF
    /// header (may be NULL)
    attach_list_offset: BigEndian<Offset16<AttachList>>,
    /// Offset to ligature caret list table, from beginning of GDEF
    /// header (may be NULL)
    lig_caret_list_offset: BigEndian<Offset16<LigCaretList>>,
    /// Offset to class definition table for mark attachment type, from
    /// beginning of GDEF header (may be NULL)
    mark_attach_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to the table of mark glyph set definitions, from
    /// beginning of GDEF header (may be NULL)
    mark_glyph_sets_def_offset: BigEndian<Offset16<MarkGlyphSets>>,
}

/// [GDEF](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#gdef-header) 1.3
#[offset_host]
Gdef1_3<'a> {
    /// Major version of the GDEF table, = 1
    major_version: BigEndian<u16>,
    /// Minor version of the GDEF table, = 3
    minor_version: BigEndian<u16>,
    /// Offset to class definition table for glyph type, from beginning
    /// of GDEF header (may be NULL)
    glyph_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to attachment point list table, from beginning of GDEF
    /// header (may be NULL)
    attach_list_offset: BigEndian<Offset16<AttachList>>,
    /// Offset to ligature caret list table, from beginning of GDEF
    /// header (may be NULL)
    lig_caret_list_offset: BigEndian<Offset16<LigCaretList>>,
    /// Offset to class definition table for mark attachment type, from
    /// beginning of GDEF header (may be NULL)
    mark_attach_class_def_offset: BigEndian<Offset16<ClassDef>>,
    /// Offset to the table of mark glyph set definitions, from
    /// beginning of GDEF header (may be NULL)
    mark_glyph_sets_def_offset: BigEndian<Offset16<MarkGlyphSets>>,
    /// Offset to the Item Variation Store table, from beginning of
    /// GDEF header (may be NULL)
    item_var_store_offset: BigEndian<Offset32>,
}

#[format(MajorMinor)]
#[generate_getters]
enum Gdef<'a> {
    #[version(MajorMinor::VERSION_1_0)]
    Gdef1_0(Gdef1_0<'a>),
    #[version(MajorMinor::VERSION_1_2)]
    Gdef1_2(Gdef1_2<'a>),
    #[version(MajorMinor::VERSION_1_3)]
    Gdef1_3(Gdef1_3<'a>),
}

/// Used in the [Glyph Class Definition Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#glyph-class-definition-table)
#[repr(u16)]
enum GlyphClassDef {
    Base = 1,
    Ligature = 2,
    Mark = 3,
    Component = 4,
}

/// [Attachment Point List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#attachment-point-list-table)
#[offset_host]
AttachList<'a> {
    /// Offset to Coverage table - from beginning of AttachList table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Number of glyphs with attachment points
    glyph_count: BigEndian<u16>,
    /// Array of offsets to AttachPoint tables-from beginning of
    /// AttachList table-in Coverage Index order
    #[count(glyph_count)]
    attach_point_offsets: [BigEndian<Offset16<AttachPoint>>],
}

/// Part of [AttachList]
AttachPoint<'a> {
    /// Number of attachment points on this glyph
    point_count: BigEndian<u16>,
    /// Array of contour point indices -in increasing numerical order
    #[count(point_count)]
    point_indices: [BigEndian<u16>],
}

/// [Ligature Caret List Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#ligature-caret-list-table)
#[offset_host]
LigCaretList<'a> {
    /// Offset to Coverage table - from beginning of LigCaretList table
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    /// Number of ligature glyphs
    lig_glyph_count: BigEndian<u16>,
    /// Array of offsets to LigGlyph tables, from beginning of
    /// LigCaretList table —in Coverage Index order
    #[count(lig_glyph_count)]
    lig_glyph_offsets: [BigEndian<Offset16<LigGlyph>>],
}

/// [Ligature Glyph Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#ligature-glyph-table)
#[offset_host]
LigGlyph<'a> {
    /// Number of CaretValue tables for this ligature (components - 1)
    caret_count: BigEndian<u16>,
    /// Array of offsets to CaretValue tables, from beginning of
    /// LigGlyph table — in increasing coordinate order
    #[count(caret_count)]
    caret_value_offsets: [BigEndian<Offset16<CaretValue>>],
}

/// [Caret Value Tables](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caret-value-tables)
#[format(u16)]
enum CaretValue<'a> {
    #[version(1)]
    Format1(CaretValueFormat1),
    #[version(2)]
    Format2(CaretValueFormat2),
    #[version(3)]
    Format3(CaretValueFormat3<'a>),
}

/// [CaretValue Format 1](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caretvalue-format-1)
CaretValueFormat1 {
    /// Format identifier: format = 1
    caret_value_format: BigEndian<u16>,
    /// X or Y value, in design units
    coordinate: BigEndian<i16>,
}

/// [CaretValue Format 2](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caretvalue-format-2)
CaretValueFormat2 {
    /// Format identifier: format = 2
    caret_value_format: BigEndian<u16>,
    /// Contour point index on glyph
    caret_value_point_index: BigEndian<u16>,
}

/// [CaretValue Format 3](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#caretvalue-format-3)
#[offset_host]
CaretValueFormat3<'a> {
    /// Format identifier-format = 3
    caret_value_format: BigEndian<u16>,
    /// X or Y value, in design units
    coordinate: BigEndian<i16>,
    /// Offset to Device table (non-variable font) / Variation Index
    /// table (variable font) for X or Y value-from beginning of
    /// CaretValue table
    device_offset: BigEndian<Offset16>,
}

/// [Mark Glyph Sets Table](https://docs.microsoft.com/en-us/typography/opentype/spec/gdef#mark-glyph-sets-table)
#[offset_host]
MarkGlyphSets<'a> {
    /// Format identifier == 1
    format: BigEndian<u16>,
    /// Number of mark glyph sets defined
    mark_glyph_set_count: BigEndian<u16>,
    /// Array of offsets to mark glyph set coverage tables, from the
    /// start of the MarkGlyphSets table.
    #[count(mark_glyph_set_count)]
    coverage_offsets: [BigEndian<Offset32>],
}
