// path (from compile crate) to the generated parse module for this table.
#![parse_module(read_fonts::tables::gsub)]
#![sanitize]

/// [GSUB](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#gsub-header)
#[tag = "GSUB"]
table Gsub {
    /// The major and minor version of the GSUB table, as a tuple (u16, u16)
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,
    /// Offset to ScriptList table, from beginning of GSUB table
    script_list_offset: Offset16<ScriptList>,
    /// Offset to FeatureList table, from beginning of GSUB table
    feature_list_offset: Offset16<FeatureList>,
    /// Offset to LookupList table, from beginning of GSUB table
    lookup_list_offset: Offset16<SubstitutionLookupList>,
    /// Offset to FeatureVariations table, from beginning of the GSUB
    /// table (may be NULL)
    #[since_version(1.1)]
    #[nullable]
    feature_variations_offset: Offset32<FeatureVariations>,
}

/// A [GSUB Lookup](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#gsubLookupTypeEnum) subtable.
 group SubstitutionLookup(Lookup, $lookup_type) {
    1 => Single(SingleSubst),
    2 => Multiple(MultipleSubstFormat1),
    3 => Alternate(AlternateSubstFormat1),
    4 => Ligature(LigatureSubstFormat1),
    5 => Contextual(SubstitutionSequenceContext),
    6 => ChainContextual(SubstitutionChainContext),
    7 => Extension(ExtensionSubtable),
    8 => Reverse(ReverseChainSingleSubstFormat1),
}

/// LookupType 1: [Single Substitution](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#lookuptype-1-single-substitution-subtable) Subtable
format u16 SingleSubst {
    Format1(SingleSubstFormat1),
    Format2(SingleSubstFormat2),
}

/// [Single Substitution Format 1](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#11-single-substitution-format-1)
table SingleSubstFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    subst_format: u16,
    /// Offset to Coverage table, from beginning of substitution
    /// subtable
    coverage_offset: Offset16<CoverageTable>,
    /// Add to original glyph ID to get substitute glyph ID
    delta_glyph_id: i16,
}

/// [Single Substitution Format 2](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#12-single-substitution-format-2)
table SingleSubstFormat2 {
    /// Format identifier: format = 2
    #[format = 2]
    subst_format: u16,
    /// Offset to Coverage table, from beginning of substitution
    /// subtable
    coverage_offset: Offset16<CoverageTable>,
    /// Number of glyph IDs in the substituteGlyphIDs array
    #[compile(array_len($substitute_glyph_ids))]
    glyph_count: u16,
    /// Array of substitute glyph IDs — ordered by Coverage index
    #[count($glyph_count)]
    substitute_glyph_ids: [GlyphId16],
}

/// [Multiple Substitution Format 1](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#21-multiple-substitution-format-1)
table MultipleSubstFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    subst_format: u16,
    /// Offset to Coverage table, from beginning of substitution
    /// subtable
    coverage_offset: Offset16<CoverageTable>,
    /// Number of Sequence table offsets in the sequenceOffsets array
    #[compile(array_len($sequence_offsets))]
    sequence_count: u16,
    /// Array of offsets to Sequence tables. Offsets are from beginning
    /// of substitution subtable, ordered by Coverage index
    #[count($sequence_count)]
    sequence_offsets: [Offset16<Sequence>],
}

/// Part of [MultipleSubstFormat1]
table Sequence {
    /// Number of glyph IDs in the substituteGlyphIDs array. This must
    /// always be greater than 0.
    #[compile(array_len($substitute_glyph_ids))]
    glyph_count: u16,
    /// String of glyph IDs to substitute
    #[count($glyph_count)]
    substitute_glyph_ids: [GlyphId16],
}

/// [Alternate Substitution Format 1](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#31-alternate-substitution-format-1)
table AlternateSubstFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    subst_format: u16,
    /// Offset to Coverage table, from beginning of substitution
    /// subtable
    coverage_offset: Offset16<CoverageTable>,
    /// Number of AlternateSet tables
    #[compile(array_len($alternate_set_offsets))]
    alternate_set_count: u16,
    /// Array of offsets to AlternateSet tables. Offsets are from
    /// beginning of substitution subtable, ordered by Coverage index
    #[count($alternate_set_count)]
    alternate_set_offsets: [Offset16<AlternateSet>],
}

/// Part of [AlternateSubstFormat1]
table AlternateSet {
    /// Number of glyph IDs in the alternateGlyphIDs array
    #[compile(array_len($alternate_glyph_ids))]
    glyph_count: u16,
    /// Array of alternate glyph IDs, in arbitrary order
    #[count($glyph_count)]
    alternate_glyph_ids: [GlyphId16],
}

/// [Ligature Substitution Format 1](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#41-ligature-substitution-format-1)
table LigatureSubstFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    subst_format: u16,
    /// Offset to Coverage table, from beginning of substitution
    /// subtable
    coverage_offset: Offset16<CoverageTable>,
    /// Number of LigatureSet tables
    #[compile(array_len($ligature_set_offsets))]
    ligature_set_count: u16,
    /// Array of offsets to LigatureSet tables. Offsets are from
    /// beginning of substitution subtable, ordered by Coverage index
    #[count($ligature_set_count)]
    ligature_set_offsets: [Offset16<LigatureSet>],
}

/// Part of [LigatureSubstFormat1]
table LigatureSet {
    /// Number of Ligature tables
    #[compile(array_len($ligature_offsets))]
    ligature_count: u16,
    /// Array of offsets to Ligature tables. Offsets are from beginning
    /// of LigatureSet table, ordered by preference.
    #[count($ligature_count)]
    ligature_offsets: [Offset16<Ligature>],
}

/// Part of [LigatureSubstFormat1]
table Ligature {
    /// glyph ID of ligature to substitute
    ligature_glyph: GlyphId16,
    /// Number of components in the ligature
    #[compile(plus_one($component_glyph_ids.len()))]
    component_count: u16,
    /// Array of component glyph IDs — start with the second
    /// component, ordered in writing direction
    #[count(subtract($component_count, 1))]
    component_glyph_ids: [GlyphId16],
}

/// [Extension Substitution Subtable Format 1](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#71-extension-substitution-subtable-format-1)
#[generic_offset(T)]
#[skip_font_write]
table ExtensionSubstFormat1 {
    /// Format identifier. Set to 1.
    #[format = 1]
    subst_format: u16,
    /// Lookup type of subtable referenced by extensionOffset (that is,
    /// the extension subtable).
    extension_lookup_type: u16,
    /// Offset to the extension subtable, of lookup type
    /// extensionLookupType, relative to the start of the
    /// ExtensionSubstFormat1 subtable.
    extension_offset: Offset32<T>,
}

/// A [GSUB Extension Substitution](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#ES) subtable
 group ExtensionSubtable(ExtensionSubstFormat1, $extension_lookup_type) {
    1 => Single(SingleSubst),
    2 => Multiple(MultipleSubstFormat1),
    3 => Alternate(AlternateSubstFormat1),
    4 => Ligature(LigatureSubstFormat1),
    5 => Contextual(SubstitutionSequenceContext),
    6 => ChainContextual(SubstitutionChainContext),
    8 => Reverse(ReverseChainSingleSubstFormat1),
}

/// [Reverse Chaining Contextual Single Substitution Format 1](https://learn.microsoft.com/en-us/typography/opentype/spec/gsub#81-reverse-chaining-contextual-single-substitution-format-1-coverage-based-glyph-contexts)
table ReverseChainSingleSubstFormat1 {
    /// Format identifier: format = 1
    #[format = 1]
    subst_format: u16,
    /// Offset to Coverage table, from beginning of substitution
    /// subtable.
    coverage_offset: Offset16<CoverageTable>,
    /// Number of glyphs in the backtrack sequence.
    #[compile(array_len($backtrack_coverage_offsets))]
    backtrack_glyph_count: u16,
    /// Array of offsets to coverage tables in backtrack sequence, in
    /// glyph sequence order.
    #[count($backtrack_glyph_count)]
    backtrack_coverage_offsets: [Offset16<CoverageTable>],
    /// Number of glyphs in lookahead sequence.
    #[compile(array_len($lookahead_coverage_offsets))]
    lookahead_glyph_count: u16,
    /// Array of offsets to coverage tables in lookahead sequence, in
    /// glyph sequence order.
    #[count($lookahead_glyph_count)]
    lookahead_coverage_offsets: [Offset16<CoverageTable>],
    /// Number of glyph IDs in the substituteGlyphIDs array.
    #[compile(array_len($substitute_glyph_ids))]
    glyph_count: u16,
    /// Array of substitute glyph IDs — ordered by Coverage index.
    #[count($glyph_count)]
    substitute_glyph_ids: [GlyphId16],
}

