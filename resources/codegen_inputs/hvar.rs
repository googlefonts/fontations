#![parse_module(read_fonts::tables::hvar)]

/// The [HVAR (Horizontal Metrics Variations)](https://docs.microsoft.com/en-us/typography/opentype/spec/hvar) table
table Hvar {
    /// Major version number of the horizontal metrics variations table — set to 1.
    /// Minor version number of the horizontal metrics variations table — set to 0.
    #[version]
    version: MajorMinor,
    /// Offset in bytes from the start of this table to the item variation store table.
    item_variation_store_offset: Offset32<ItemVariationStore>,
    #[nullable]
    /// Offset in bytes from the start of this table to the delta-set index mapping for advance widths (may be NULL).
    advance_width_mapping_offset: Offset32<DeltaSetIndexMap>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for left side bearings (may be NULL).
    #[nullable]
    lsb_mapping_offset: Offset32<DeltaSetIndexMap>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for right side bearings (may be NULL).
    #[nullable]
    rsb_mapping_offset: Offset32<DeltaSetIndexMap>,
}
