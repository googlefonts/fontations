#![parse_module(read_fonts::tables::morx)]

/// The [morx (Extended Glyph Metamorphosis)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6morx.html) table.
#[tag = "morx"]
table Morx {
    /// Version number of the extended glyph metamorphosis table (either 2 or 3).
    version: u16,
    /// Not used; set to 0.
    #[skip_getter]
    #[compile(0)]
    unused: u16,
    /// Number of metamorphosis chains contained in this table.
    n_chains: u32,
    #[count($n_chains)]
    chains: VarLenArray<Chain<'a>>,
}

/// A chain in a `morx` table.
table Chain {
    /// The default specification for subtables.
    default_flags: u32,
    /// Total byte count, including this header; must be a multiple of 4.
    chain_length: u32,
    /// Number of feature subtable entries.
    n_feature_entries: u32,
    /// The number of subtables in the chain.
    n_subtables: u32,
    /// Feature entries for this chain.
    #[count($n_feature_entries)]
    features: [Feature],
    /// Array of chain subtables.
    #[count($n_subtables)]
    subtables: VarLenArray<Subtable<'a>>,
}

/// Used to compute the sub-feature flags for a list of requested features and settings.
record Feature {
    /// The type of feature.
    feature_type: u16,
    /// The feature's setting (aka selector).
    feature_settings: u16,
    /// Flags for the settings that this feature and setting enables.
    enable_flags: u32,
    /// Complement of flags for the settings that this feature and setting disable.
    disable_flags: u32,
}

/// A subtable in a `morx` chain.
table Subtable {
    /// Total subtable length, including this header.
    length: u32,    
    /// Coverage flags and subtable type.
    coverage: u32,
    /// The 32-bit mask identifying which subtable this is (the subtable being executed if the AND of this value and the processed defaultFlags is nonzero).
    sub_feature_flags: u32,
    /// Data for specific subtable.
    #[count(..)]
    data: [u8],
}

/// Entry payload in a contextual subtable state machine.
record ContextualEntryData {
    /// Index of the substitution table for the marked glyph (use 0xFFFF for
    /// none).
    mark_index: u16,
    /// Index of the substitution table for the current glyph (use 0xFFFF for
    /// none)
    current_index: u16,
}

/// Entry payload in an insertion subtable state machine.
record InsertionEntryData {
    /// Zero-based index into the insertion glyph table. The number of glyphs
    /// to be inserted is contained in the currentInsertCount field in the
    /// flags (see below). A value of 0xFFFF indicates no insertion is to be done.
    current_insert_index: u16,
    /// Zero-based index into the insertion glyph table. The number of glyphs
    /// to be inserted is contained in the markedInsertCount field in the
    /// flags (see below). A value of 0xFFFF indicates no insertion is to be
    /// done.
    marked_insert_index: u16,
}
