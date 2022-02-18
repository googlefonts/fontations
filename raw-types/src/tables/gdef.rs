use crate::{Int16, Offset16, Offset32, Uint16};

toy_table_macro::tables! {
    Gdef_1_0 {
        /// Major version of the GDEF table, = 1
        major_version: Uint16,
        /// Minor version of the GDEF table, = 0
        minor_version: Uint16,
        /// Offset to class definition table for glyph type, from beginning
        /// of GDEF header (may be NULL)
        glyph_class_def_offset: Offset16,
        /// Offset to attachment point list table, from beginning of GDEF
        /// header (may be NULL)
        attach_list_offset: Offset16,
        /// Offset to ligature caret list table, from beginning of GDEF
        /// header (may be NULL)
        lig_caret_list_offset: Offset16,
        /// Offset to class definition table for mark attachment type, from
        /// beginning of GDEF header (may be NULL)
        mark_attach_class_def_offset: Offset16,
    }

    Gdef_1_2 {
        /// Major version of the GDEF table, = 1
        major_version: Uint16,
        /// Minor version of the GDEF table, = 2
        minor_version: Uint16,
        /// Offset to class definition table for glyph type, from beginning
        /// of GDEF header (may be NULL)
        glyph_class_def_offset: Offset16,
        /// Offset to attachment point list table, from beginning of GDEF
        /// header (may be NULL)
        attach_list_offset: Offset16,
        /// Offset to ligature caret list table, from beginning of GDEF
        /// header (may be NULL)
        lig_caret_list_offset: Offset16,
        /// Offset to class definition table for mark attachment type, from
        /// beginning of GDEF header (may be NULL)
        mark_attach_class_def_offset: Offset16,
        /// Offset to the table of mark glyph set definitions, from
        /// beginning of GDEF header (may be NULL)
        mark_glyph_sets_def_offset: Offset16,
    }

    Gdef_1_3 {
        /// Major version of the GDEF table, = 1
        major_version: Uint16,
        /// Minor version of the GDEF table, = 3
        minor_version: Uint16,
        /// Offset to class definition table for glyph type, from beginning
        /// of GDEF header (may be NULL)
        glyph_class_def_offset: Offset16,
        /// Offset to attachment point list table, from beginning of GDEF
        /// header (may be NULL)
        attach_list_offset: Offset16,
        /// Offset to ligature caret list table, from beginning of GDEF
        /// header (may be NULL)
        lig_caret_list_offset: Offset16,
        /// Offset to class definition table for mark attachment type, from
        /// beginning of GDEF header (may be NULL)
        mark_attach_class_def_offset: Offset16,
        /// Offset to the table of mark glyph set definitions, from
        /// beginning of GDEF header (may be NULL)
        mark_glyph_sets_def_offset: Offset16,
        /// Offset to the Item Variation Store table, from beginning of
        /// GDEF header (may be NULL)
        item_var_store_offset: Offset32,
    }
}

toy_table_macro::tables! {
AttachList<'a> {
    /// Offset to Coverage table - from beginning of AttachList table
    coverage_offset: Offset16,
    /// Number of glyphs with attachment points
    glyph_count: Uint16,
    /// Array of offsets to AttachPoint tables-from beginning of
    /// AttachList table-in Coverage Index order
    #[count(glyph_count)]
    attach_point_offsets: [Offset16],
}

AttachPoint<'a> {
    /// Number of attachment points on this glyph
    point_count: Uint16,
    /// Array of contour point indices -in increasing numerical order
    #[count(point_count)]
    point_indices: [Uint16],
}
}

toy_table_macro::tables! {
    LigCaretList<'a> {
        /// Offset to Coverage table - from beginning of LigCaretList table
        coverage_offset: Offset16,
        /// Number of ligature glyphs
        lig_glyph_count: Uint16,
        /// Array of offsets to LigGlyph tables, from beginning of
        /// LigCaretList table —in Coverage Index order
        #[count(lig_glyph_count)]
        lig_glyph_offsets: [Offset16],
    }

    LigGlyph<'a> {
        /// Number of CaretValue tables for this ligature (components - 1)
        caret_count: Uint16,
        /// Array of offsets to CaretValue tables, from beginning of
        /// LigGlyph table — in increasing coordinate order
        #[count(caret_count)]
        caret_value_offsets: [Offset16],
    }

    CaretValueFormat1 {
        /// Format identifier: format = 1
        caret_value_format: Uint16,
        /// X or Y value, in design units
        coordinate: Int16,
    }

    CaretValueFormat2 {
        /// Format identifier: format = 2
        caret_value_format: Uint16,
        /// Contour point index on glyph
        caret_value_point_index: Uint16,
    }

    CaretValueFormat3 {
        /// Format identifier-format = 3
        caret_value_format: Uint16,
        /// X or Y value, in design units
        coordinate: Int16,
        /// Offset to Device table (non-variable font) / Variation Index
        /// table (variable font) for X or Y value-from beginning of
        /// CaretValue table
        device_offset: Offset16,
    }

    #[format(Uint16)]
    enum CaretValue {
        #[version(CaretValue::FORMAT_1)]
        Format1(CaretValueFormat1),
        #[version(CaretValue::FORMAT_2)]
        Format2(CaretValueFormat2),
        #[version(CaretValue::FORMAT_3)]
        Format3(CaretValueFormat3),
    }
}

impl CaretValue {
    const FORMAT_1: Uint16 = Uint16::from_bytes(1u16.to_be_bytes());
    const FORMAT_2: Uint16 = Uint16::from_bytes(2u16.to_be_bytes());
    const FORMAT_3: Uint16 = Uint16::from_bytes(3u16.to_be_bytes());
}

toy_table_macro::tables! {
    MarkGlyphSets<'a> {
        /// Format identifier == 1
        format: Uint16,
        /// Number of mark glyph sets defined
        mark_glyph_set_count: Uint16,
        /// Array of offsets to mark glyph set coverage tables, from the
        /// start of the MarkGlyphSets table.
        #[count(mark_glyph_set_count)]
        coverage_offsets: [Offset32],
    }
}
