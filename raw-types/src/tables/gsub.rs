use crate::{Int16, Offset16, Offset32, Uint16};

toy_table_macro::tables! {
    Gsub_1_0 {
        /// Major version of the GSUB table, = 1
        major_version: Uint16,
        /// Minor version of the GSUB table, = 0
        minor_version: Uint16,
        /// Offset to ScriptList table, from beginning of GSUB table
        script_list_offset: Offset16,
        /// Offset to FeatureList table, from beginning of GSUB table
        feature_list_offset: Offset16,
        /// Offset to LookupList table, from beginning of GSUB table
        lookup_list_offset: Offset16,
    }

    Gsub_1_1 {
        /// Major version of the GSUB table, = 1
        major_version: Uint16,
        /// Minor version of the GSUB table, = 1
        minor_version: Uint16,
        /// Offset to ScriptList table, from beginning of GSUB table
        script_list_offset: Offset16,
        /// Offset to FeatureList table, from beginning of GSUB table
        feature_list_offset: Offset16,
        /// Offset to LookupList table, from beginning of GSUB table
        lookup_list_offset: Offset16,
        /// Offset to FeatureVariations table, from beginning of the GSUB
        /// table (may be NULL)
        feature_variations_offset: Offset32,
    }

    SingleSubstFormat1 {
        /// Format identifier: format = 1
        subst_format: Uint16,
        /// Offset to Coverage table, from beginning of substitution
        /// subtable
        coverage_offset: Offset16,
        /// Add to original glyph ID to get substitute glyph ID
        delta_glyph_i_d: Int16,
    }

    SingleSubstFormat2<'a> {
        /// Format identifier: format = 2
        subst_format: Uint16,
        /// Offset to Coverage table, from beginning of substitution
        /// subtable
        coverage_offset: Offset16,
        /// Number of glyph IDs in the substituteGlyphIDs array
        glyph_count: Uint16,
        /// Array of substitute glyph IDs — ordered by Coverage index
        #[count(glyph_count)]
        substitute_glyph_i_ds: [Uint16],
    }

    MultipleSubstFormat1<'a> {
        /// Format identifier: format = 1
        subst_format: Uint16,
        /// Offset to Coverage table, from beginning of substitution
        /// subtable
        coverage_offset: Offset16,
        /// Number of Sequence table offsets in the sequenceOffsets array
        sequence_count: Uint16,
        /// Array of offsets to Sequence tables. Offsets are from beginning
        /// of substitution subtable, ordered by Coverage index
        #[count(sequence_count)]
        sequence_offsets: [Offset16],
    }

    Sequence<'a> {
        /// Number of glyph IDs in the substituteGlyphIDs array. This must
        /// always be greater than 0.
        glyph_count: Uint16,
        /// String of glyph IDs to substitute
        #[count(glyph_count)]
        substitute_glyph_i_ds: [Uint16],
    }

    AlternateSubstFormat1<'a> {
        /// Format identifier: format = 1
        subst_format: Uint16,
        /// Offset to Coverage table, from beginning of substitution
        /// subtable
        coverage_offset: Offset16,
        /// Number of AlternateSet tables
        alternate_set_count: Uint16,
        /// Array of offsets to AlternateSet tables. Offsets are from
        /// beginning of substitution subtable, ordered by Coverage index
        #[count(alternate_set_count)]
        alternate_set_offsets: [Offset16],
    }

    AlternateSet<'a> {
        /// Number of glyph IDs in the alternateGlyphIDs array
        glyph_count: Uint16,
        /// Array of alternate glyph IDs, in arbitrary order
        #[count(glyph_count)]
        alternate_glyph_i_ds: [Uint16],
    }

    LigatureSubstFormat1<'a> {
        /// Format identifier: format = 1
        subst_format: Uint16,
        /// Offset to Coverage table, from beginning of substitution
        /// subtable
        coverage_offset: Offset16,
        /// Number of LigatureSet tables
        ligature_set_count: Uint16,
        /// Array of offsets to LigatureSet tables. Offsets are from
        /// beginning of substitution subtable, ordered by Coverage index
        #[count(ligature_set_count)]
        ligature_set_offsets: [Offset16],
    }

    LigatureSet<'a> {
        /// Number of Ligature tables
        ligature_count: Uint16,
        /// Array of offsets to Ligature tables. Offsets are from beginning
        /// of LigatureSet table, ordered by preference.
        #[count(ligature_count)]
        ligature_offsets: [Offset16],
    }

    Ligature<'a> {
        /// glyph ID of ligature to substitute
        ligature_glyph: Uint16,
        /// Number of components in the ligature
        component_count: Uint16,
        /// - 1]    Array of component glyph IDs — start with the second
        /// component, ordered in writing direction
        #[count(component_count)]
        component_glyph_i_ds: [Uint16],
    }

    ExtensionSubstFormat1 {
        /// Format identifier. Set to 1.
        subst_format: Uint16,
        /// Lookup type of subtable referenced by extensionOffset (that is,
        /// the extension subtable).
        extension_lookup_type: Uint16,
        /// Offset to the extension subtable, of lookup type
        /// extensionLookupType, relative to the start of the
        /// ExtensionSubstFormat1 subtable.
        extension_offset: Offset32,
    }

    //FIXME: scalars after arrays
    ReverseChainSingleSubstFormat1<'a> {
        /// Format identifier: format = 1
        subst_format: Uint16,
        /// Offset to Coverage table, from beginning of substitution
        /// subtable.
        coverage_offset: Offset16,
        /// Number of glyphs in the backtrack sequence.
        backtrack_glyph_count: Uint16,
        /// Array of offsets to coverage tables in backtrack sequence, in
        /// glyph sequence order.
        #[count(backtrack_glyph_count)]
        backtrack_coverage_offsets: [Offset16],
        /// Number of glyphs in lookahead sequence.
        lookahead_glyph_count: Uint16,
        /// Array of offsets to coverage tables in lookahead sequence, in
        /// glyph sequence order.
        //#[count(lookahead_glyph_count)]
        #[count(0)]
        lookahead_coverage_offsets: [Offset16],
        /// Number of glyph IDs in the substituteGlyphIDs array.
        glyph_count: Uint16,
        /// Array of substitute glyph IDs — ordered by Coverage index.
        //#[count(glyph_count)]
        #[count(0)]
        substitute_glyph_i_ds: [Uint16],
    }

}
