#![parse_module(read_fonts::tables::mort)]

/// The [mort (Glyph Metamorphosis)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6mort.html) table.
#[tag = "mort"]
table Mort {
    /// Version number of the glyph metamorphosis table.
    version: u16,
    /// Not used; set to 0.
    #[skip_getter]
    #[compile(0)]
    unused: u16,
    /// Number of metamorphosis chains contained in this table.
    #[compile(array_len($chains))]
    n_chains: u32,
    #[count($n_chains)]
    chains: VarLenArray<Chain<'a>>,
}

/// A chain in a `mort` table.
table Chain {
    /// The default specification for subtables.
    default_flags: u32,
    /// Total byte count, including this header.
    #[compile(self.compute_chain_length())]
    chain_length: u32,
    /// Number of feature subtable entries.
    #[compile(array_len($features))]
    n_feature_entries: u16,
    /// The number of subtables in the chain.
    n_subtables: u16,
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

/// A subtable in a `mort` chain.
table Subtable {
    /// Total subtable length, including this header.
    length: u16,
    /// Coverage flags and subtable type.
    coverage: u16,
    /// The 32-bit mask identifying which subtable this is.
    sub_feature_flags: u32,
    /// Data for the specific subtable type.
    #[count(..)]
    data: [u8],
}

/// Entry payload in a contextual subtable state machine.
record ContextualEntryData {
    /// Signed word offset of the substitution array for the marked glyph (0 for none).
    mark_offset: i16,
    /// Signed word offset of the substitution array for the current glyph (0 for none).
    current_offset: i16,
}

/// Entry payload in an insertion subtable state machine.
record InsertionEntryData {
    /// Zero-based index into the insertion glyph table (0xFFFF for none).
    current_insert_index: u16,
    /// Zero-based index into the insertion glyph table (0xFFFF for none).
    marked_insert_index: u16,
}
